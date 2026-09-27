//! Feature 028, the accelerated inference spike (spec, research D3–D5): the compute path this
//! build was compiled for, and the device the embedder is built on. Chosen at compile time by a
//! non-default feature, never at run time — an environment variable must not be able to change a
//! number (`quantised_bert`'s rule). The embedder's identity is the same on every path (research
//! D6), so an index built on one path opens on another; the spike's records name the path instead.
//!
//! Spike code: removed or promoted by the follow-up feature the spike's verdict names (FR-012).

use candle_core::Device;
use xtriever_core::Result;

#[cfg(all(feature = "spike-accelerate", feature = "spike-metal"))]
compile_error!("the spike features `spike-accelerate` and `spike-metal` are mutually exclusive");

/// The compute path of this build: `"cpu"` (the default), `"accelerate"` (Apple's matrix
/// library for the CPU's float matrix multiply) or `"metal"` (the Apple GPU).
#[cfg(not(any(feature = "spike-accelerate", feature = "spike-metal")))]
pub const COMPUTE_PATH: &str = "cpu";
/// The compute path of this build: `"cpu"` (the default), `"accelerate"` (Apple's matrix
/// library for the CPU's float matrix multiply) or `"metal"` (the Apple GPU).
#[cfg(feature = "spike-accelerate")]
pub const COMPUTE_PATH: &str = "accelerate";
/// The compute path of this build: `"cpu"` (the default), `"accelerate"` (Apple's matrix
/// library for the CPU's float matrix multiply) or `"metal"` (the Apple GPU).
#[cfg(feature = "spike-metal")]
pub const COMPUTE_PATH: &str = "metal";

/// The device the embedder is built on: the CPU, unless this is a `spike-metal` build. Apple's
/// matrix library needs no device of its own — the feature reroutes candle's CPU matrix
/// multiply (research D3).
///
/// # Errors
///
/// `Error::Model` naming `spike-metal` and candle's message when the GPU cannot be opened.
/// There is no fallback to the CPU: the spike records the failure (spec Edge Cases).
pub(crate) fn compute_device() -> Result<Device> {
    #[cfg(feature = "spike-metal")]
    {
        Device::new_metal(0)
            .map_err(|e| crate::error::model_err(format!("spike-metal: cannot open the GPU: {e}")))
    }
    #[cfg(not(feature = "spike-metal"))]
    {
        Ok(Device::Cpu)
    }
}
