use super::{
    error,
    Result,
};

pub(super) fn get(_account: &str) -> Result<Option<String>> {
    Err(error(
        "Background notifications require the Pipedash mobile app",
    ))
}

pub(super) fn set(_account: &str, _value: &str) -> Result<()> {
    Err(error(
        "Background notifications require the Pipedash mobile app",
    ))
}

pub(super) fn remove(_account: &str) -> Result<()> {
    Err(error(
        "Background notifications require the Pipedash mobile app",
    ))
}
