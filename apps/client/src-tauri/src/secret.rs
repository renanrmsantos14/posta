//! Senhas das contas. Desktop: cofre do sistema (crate `keyring`). Android: Android Keystore
//! (crate `android-native-keyring-store`, que cifra um SharedPreferences com chave do Keystore).

const SERVICE: &str = "posta";

#[cfg(not(target_os = "android"))]
mod imp {
    use super::SERVICE;

    pub fn set(id: &str, password: &str) -> Result<(), String> {
        keyring::Entry::new(SERVICE, id).and_then(|e| e.set_password(password)).map_err(|e| e.to_string())
    }

    pub fn get(id: &str) -> Result<String, String> {
        keyring::Entry::new(SERVICE, id).and_then(|e| e.get_password()).map_err(|e| e.to_string())
    }
}

#[cfg(target_os = "android")]
mod imp {
    use super::SERVICE;
    use keyring_core::Entry;
    use std::sync::OnceLock;

    /// O Tauri já inicializa o `ndk-context`; falta só registrar o cofre na primeira chamada.
    fn entry(id: &str) -> Result<Entry, String> {
        static READY: OnceLock<Result<(), String>> = OnceLock::new();
        READY
            .get_or_init(|| {
                let store = android_native_keyring_store::Store::new().map_err(|e| e.to_string())?;
                keyring_core::set_default_store(store);
                Ok(())
            })
            .clone()?;
        Entry::new(SERVICE, id).map_err(|e| e.to_string())
    }

    pub fn set(id: &str, password: &str) -> Result<(), String> {
        entry(id)?.set_password(password).map_err(|e| e.to_string())
    }

    pub fn get(id: &str) -> Result<String, String> {
        entry(id)?.get_password().map_err(|e| e.to_string())
    }
}

pub use imp::{get, set};
