package dev.xtriever.android

import java.io.File

/**
 * Whether this processor can run the native library at all (Feature 025, spec FR-002;
 * `docs/adr/0014-android-half-precision-floor.md`).
 *
 * The engine's matrix kernel is compiled with ARMv8.2 half-precision instructions, which are
 * optional in the base 64-bit ARM profile. A processor without them does not fail politely:
 * the instruction is decoded at run time and the process dies with an illegal-instruction
 * signal that no `catch` can reach. The only way to give a person a sentence instead of a
 * crash is to look before leaping, which is what this does — once, cheaply, before any native
 * call.
 */
sealed class DeviceSupport {
    /** The processor reports both required features. */
    object Supported : DeviceSupport()

    /** It does not; [reason] names what is missing, in words a person can act on. */
    data class Unsupported(val reason: String) : DeviceSupport()

    companion object {
        private const val SCALAR = "fphp"
        private const val VECTOR = "asimdhp"

        /**
         * Reads the processor's advertised features. Returns [Supported] only when both
         * half-precision features are present.
         */
        fun check(cpuinfo: File = File("/proc/cpuinfo")): DeviceSupport {
            val features = try {
                cpuinfo.useLines { lines ->
                    lines.firstOrNull { it.startsWith("Features") }?.substringAfter(':')?.trim()
                }
            } catch (e: Exception) {
                return Unsupported("cannot read ${cpuinfo.path}: ${e.message}")
            } ?: return Unsupported("${cpuinfo.path} lists no processor features")

            val present = features.split(' ').toSet()
            val missing = listOf(SCALAR, VECTOR).filterNot { it in present }
            return if (missing.isEmpty()) {
                Supported
            } else {
                Unsupported(
                    "this processor lacks ${missing.joinToString(" and ")}: Xtriever needs ARMv8.2 " +
                        "half-precision floating point, which every 64-bit device from about 2018 has",
                )
            }
        }
    }
}
