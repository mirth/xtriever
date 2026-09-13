import Darwin
import Foundation

/// Memory and wall-time measurement for the on-device run (FR-017 – FR-020).
///
/// Everything here uses **public** APIs. Research R1 anticipated that
/// `proc_pid_rusage` / `ri_lifetime_max_phys_footprint` — the peak counter shown in WWDC22's
/// memory session — is declared in no public iOS SDK header, and that is confirmed: it does not
/// resolve from Swift for `arm64-apple-ios`. Rather than `dlsym` an undeclared symbol, the peak is
/// taken from `task_vm_info` (see ``Snapshot/ledgerPeakBytes``) and cross-checked against sampling.
/// ``Measurement/peakMethod`` records which number the verdict used, as R1 requires.
public enum Measure {

    /// One reading of the process's memory state.
    public struct Snapshot {
        /// `phys_footprint` — the metric iOS enforces when deciding to terminate an app.
        ///
        /// Not RSS: footprint counts dirty + compressed + swapped pages. WWDC22 "Profile and
        /// optimize your game's memory": *"Memory footprint … is used for memory limit enforcement."*
        public let footprintBytes: UInt64

        /// `ledger_phys_footprint_peak` — a high-water mark maintained by the kernel.
        ///
        /// Present in `task_vm_info` since rev3. Apple documents the field's existence but not its
        /// semantics, so it is reported *alongside* the sampled maximum rather than instead of it.
        public let ledgerPeakBytes: UInt64

        /// Bytes remaining before this process hits its dirty-memory limit.
        public let availableBytes: UInt64

        /// `false` when `task_info` failed, so the footprint fields are not real readings.
        ///
        /// Load-bearing: without it a failed syscall reports 0 bytes, and a run maximum computed
        /// over zeros is a **false PASS** against the constitution's ceiling — the single worst thing this
        /// harness could do (FR-028).
        public let isValid: Bool

        /// The process's own limit, derived as footprint + remaining.
        ///
        /// Apple publishes no per-device limits and does not document this sum as a supported way
        /// to obtain one, so it is recorded as an observation. It is a **different threshold** from
        /// the constitution's RSS ceiling and must never be conflated with it.
        public var observedLimitBytes: UInt64 { footprintBytes &+ availableBytes }
    }

    /// Number of `natural_t` words up to and including `phys_footprint`.
    ///
    /// The Swift equivalent of C's `TASK_VM_INFO_REV1_COUNT`, which is not importable.
    private static let rev1Count: mach_msg_type_number_t = {
        guard let offset = MemoryLayout<task_vm_info_data_t>.offset(of: \.phys_footprint) else {
            // Fall back to requiring the whole struct rather than silently accepting a short reply.
            return mach_msg_type_number_t(
                MemoryLayout<task_vm_info_data_t>.size / MemoryLayout<natural_t>.size
            )
        }
        let bytes = offset + MemoryLayout<mach_vm_size_t>.size
        return mach_msg_type_number_t(bytes / MemoryLayout<natural_t>.size)
    }()

    /// Read the current memory state.
    public static func snapshot() -> Snapshot {
        var info = task_vm_info_data_t()
        var count = mach_msg_type_number_t(
            MemoryLayout<task_vm_info_data_t>.size / MemoryLayout<natural_t>.size
        )
        let result = withUnsafeMutablePointer(to: &info) {
            $0.withMemoryRebound(to: integer_t.self, capacity: Int(count)) {
                task_info(mach_task_self_, task_flavor_t(TASK_VM_INFO), $0, &count)
            }
        }
        // `phys_footprint` arrived in revision 1 of this struct, so a shorter reply means the
        // field is not populated and must not be read as a real zero. `TASK_VM_INFO_REV1_COUNT` is
        // a C macro built from struct offsets and Swift does not import it, so the equivalent
        // threshold is computed from the field's own offset.
        guard result == KERN_SUCCESS, count >= Self.rev1Count else {
            return Snapshot(footprintBytes: 0, ledgerPeakBytes: 0,
                            availableBytes: UInt64(os_proc_available_memory()), isValid: false)
        }
        return Snapshot(
            footprintBytes: UInt64(info.phys_footprint),
            ledgerPeakBytes: UInt64(max(0, info.ledger_phys_footprint_peak)),
            availableBytes: UInt64(os_proc_available_memory()),
            isValid: true
        )
    }

    /// Monotonic nanoseconds.
    ///
    /// Apple's own `mach_absolute_time` reference says *"Prefer to use the equivalent
    /// `clock_gettime_nsec_np(CLOCK_UPTIME_RAW)`"*. Chosen over Swift's `ContinuousClock` because
    /// that keeps counting while the system sleeps, which is wrong for attributing compute time.
    public static func nowNanos() -> UInt64 { clock_gettime_nsec_np(CLOCK_UPTIME_RAW) }

    /// Which peak number a measurement reported.
    public enum PeakMethod: String, Codable {
        /// `task_vm_info.ledger_phys_footprint_peak`.
        case ledger
        /// Maximum of footprint samples taken around the operation.
        case sampled
    }

    /// One measured operation (data-model.md `Measurement`).
    public struct Measurement: Codable {
        public let operation: String
        public let wallNanos: UInt64
        /// Footprint immediately after the operation.
        public let footprintBytes: UInt64
        /// Change in footprint across the operation — what this operation itself cost.
        public let footprintDeltaBytes: Int64
        /// Process-**lifetime** high-water mark as of the end of this operation.
        ///
        /// Deliberately *not* called a per-operation peak. `ledger_phys_footprint_peak` is
        /// monotonic across the process, so this value includes every operation that ran before
        /// it. Only the largest value across a run is meaningful as "the run's peak"; reading a
        /// single row as "this operation peaked at X" would be wrong.
        public let cumulativePeakBytes: UInt64
        /// Which source produced ``cumulativePeakBytes``.
        public let peakMethod: PeakMethod
        public let ledgerPeakBytes: UInt64
        public let sampledPeakBytes: UInt64
        /// `os_proc_available_memory()` before/after. **Zero on the Simulator**, which Apple
        /// documents as returning 0 for a non-app process — another reason simulator numbers are
        /// not device results.
        public let availableBefore: UInt64
        public let availableAfter: UInt64
        /// `buffered` or `mmapped`, set only for weight-loading operations (ADR-0002).
        public let loadPath: String?
        /// `false` if either memory snapshot failed. A run containing an invalid measurement has no
        /// usable peak and must be reported `untested`, never as a passing verdict.
        public let memoryIsValid: Bool

        /// Milliseconds, for humans reading the report.
        public var wallMillis: Double { Double(wallNanos) / 1_000_000 }
        /// Mebibytes, for comparing against the constitution's ceiling.
        public var cumulativePeakMiB: Double { Double(cumulativePeakBytes) / (1024 * 1024) }
    }

    /// Run `body`, timing it and recording the memory around it.
    ///
    /// ``Measurement/cumulativePeakBytes`` takes the **larger** of the kernel's ledger high-water
    /// mark and the sampled maximum — deliberately the conservative choice. Sampling only sees
    /// before and after, so it misses a transient spike inside the operation; the ledger catches
    /// those. Under-reporting the peak is the one error that would turn a real ceiling breach into
    /// a false pass (FR-028).
    public static func measure<T>(
        _ operation: String,
        loadPath: String? = nil,
        body: () throws -> T
    ) rethrows -> (value: T, measurement: Measurement) {
        let before = snapshot()
        let start = nowNanos()
        let value = try body()
        let elapsed = nowNanos() &- start
        let after = snapshot()

        let sampledPeak = max(before.footprintBytes, after.footprintBytes)
        let ledgerPeak = max(before.ledgerPeakBytes, after.ledgerPeakBytes)
        let usingLedger = ledgerPeak >= sampledPeak

        return (value, Measurement(
            operation: operation,
            wallNanos: elapsed,
            footprintBytes: after.footprintBytes,
            footprintDeltaBytes: Int64(bitPattern: after.footprintBytes)
                - Int64(bitPattern: before.footprintBytes),
            cumulativePeakBytes: max(ledgerPeak, sampledPeak),
            peakMethod: usingLedger ? .ledger : .sampled,
            ledgerPeakBytes: ledgerPeak,
            sampledPeakBytes: sampledPeak,
            availableBefore: before.availableBytes,
            availableAfter: after.availableBytes,
            loadPath: loadPath,
            memoryIsValid: before.isValid && after.isValid
        ))
    }

    /// Hardware identifier, e.g. `iPhone17,5`.
    ///
    /// `UIDevice.model` returns "iPhone" for every iPhone ever made, which is useless for a report
    /// whose whole point is that memory limits vary by device.
    public static var deviceModel: String {
        var system = utsname()
        uname(&system)
        return withUnsafePointer(to: &system.machine) {
            $0.withMemoryRebound(to: CChar.self, capacity: Int(_SYS_NAMELEN)) { String(cString: $0) }
        }
    }

    /// Thermal state at the time of the call — a throttled device gives incomparable wall times.
    public static var thermalState: String {
        switch ProcessInfo.processInfo.thermalState {
        case .nominal: return "nominal"
        case .fair: return "fair"
        case .serious: return "serious"
        case .critical: return "critical"
        @unknown default: return "unknown"
        }
    }

    /// `true` when the binary was built without optimisation.
    ///
    /// Recorded because a Debug measurement is not a measurement: research D11's harness hygiene
    /// requires Release, and a run that silently used Debug would report numbers nobody should quote.
    public static var isDebugBuild: Bool {
        #if DEBUG
        return true
        #else
        return false
        #endif
    }
}
