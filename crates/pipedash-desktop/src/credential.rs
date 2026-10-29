#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use keyring::{
    Entry,
    Error,
};
#[cfg(any(target_os = "ios", target_os = "android"))]
pub use keyring_core::{
    Entry,
    Error,
};

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[cfg(target_os = "ios")]
pub fn init() -> Result<()> {
    keyring_core::set_default_store(apple_native_keyring_store::protected::Store::new()?);
    Ok(())
}

#[cfg(target_os = "android")]
pub fn init() -> Result<()> {
    keyring_core::set_default_store(android_native_keyring_store::Store::new()?);
    Ok(())
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub fn init() -> Result<()> {
    Ok(())
}

#[cfg(target_os = "ios")]
pub fn device_only_entry(service: &str, user: &str) -> Result<Entry> {
    let modifiers =
        std::collections::HashMap::from([("access-policy", "WhenUnlockedThisDeviceOnly")]);
    Entry::new_with_modifiers(service, user, &modifiers)
}

#[cfg(not(target_os = "ios"))]
pub fn device_only_entry(service: &str, user: &str) -> Result<Entry> {
    Entry::new(service, user)
}

#[cfg(target_os = "ios")]
pub fn background_entry(service: &str, user: &str) -> Result<Entry> {
    let modifiers =
        std::collections::HashMap::from([("access-policy", "AfterFirstUnlockThisDeviceOnly")]);
    Entry::new_with_modifiers(service, user, &modifiers)
}

#[cfg(target_os = "android")]
pub fn background_entry(service: &str, user: &str) -> Result<Entry> {
    Entry::new(service, user)
}
