//! A newer host may add a value to an enum a client receives. Such an enum
//! decodes that value as `Unknown`, so one new kind never fails a whole list.
//! A request must name a value the host knows: request fields decode through
//! [`known`] and [`known_opt`], which refuse `Unknown` as a decode error.
use serde::{Deserialize, Deserializer};

/// An enum with an `Unknown` fallback.
pub trait Known {
    /// False for `Unknown`: a value this build does not know.
    fn is_known(&self) -> bool;
}

macro_rules! impl_known {
    ($($name:ty),* $(,)?) => {
        $(impl $crate::known::Known for $name {
            fn is_known(&self) -> bool {
                !matches!(self, Self::Unknown)
            }
        })*
    };
}

impl_known!(
    crate::ItemKind,
    crate::ItemState,
    crate::BasisKind,
    crate::ChangeKind,
    crate::Share,
    crate::OverallRequests,
    crate::CaptureSource,
    crate::CaptureState,
    crate::WorkState,
    crate::AssistScope,
    crate::Delivery,
    crate::SortAction,
    crate::CorrectionKind,
    crate::Confidence,
    crate::HypothesisState,
    crate::InboxEntryKind,
    crate::Disposition,
    crate::InboxAction,
    crate::LoopState,
    crate::LoopReason,
    crate::PendingPromptKind,
    crate::RunReason,
    crate::TaskStatus,
    crate::DocKind,
    crate::ChangeStatus,
    crate::ChatKind,
    crate::PromptKind,
    crate::ErrorCode,
    crate::AgentState,
    crate::Role,
);

pub(crate) fn known<'de, D, T>(d: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Known,
{
    let value = T::deserialize(d)?;
    if value.is_known() {
        Ok(value)
    } else {
        Err(serde::de::Error::custom(
            "unknown value; this muxa-bridge does not know it",
        ))
    }
}

pub(crate) fn known_opt<'de, D, T>(d: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Known,
{
    match Option::<T>::deserialize(d)? {
        Some(value) if !value.is_known() => Err(serde::de::Error::custom(
            "unknown value; this muxa-bridge does not know it",
        )),
        value => Ok(value),
    }
}
