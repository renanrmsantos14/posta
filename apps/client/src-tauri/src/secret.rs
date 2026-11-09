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

    /// Entrada que não existe não é erro: o objetivo (não ter segredo guardado) já está cumprido.
    pub fn delete(id: &str) -> Result<(), String> {
        match keyring::Entry::new(SERVICE, id).and_then(|e| e.delete_credential()) {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

#[cfg(target_os = "android")]
mod imp {
    use super::SERVICE;
    use keyring_core::Entry;
    use std::sync::OnceLock;

    /// Registra o cofre uma vez. Precisa do `ndk-context` só aqui: o Store guarda a JavaVM e o contexto.
    pub fn init() -> Result<(), String> {
        static READY: OnceLock<Result<(), String>> = OnceLock::new();
        READY
            .get_or_init(|| {
                let store = android_native_keyring_store::Store::new().map_err(|e| e.to_string())?;
                keyring_core::set_default_store(store);
                Ok(())
            })
            .clone()
    }

    fn entry(id: &str) -> Result<Entry, String> {
        init()?;
        Entry::new(SERVICE, id).map_err(|e| e.to_string())
    }

    pub fn set(id: &str, password: &str) -> Result<(), String> {
        keyring::Entry::new(SERVICE, id).and_then(|e| e.set_password(password)).map_err(|e| e.to_string())
    }

    pub fn get(id: &str) -> Result<String, String> {
        keyring::Entry::new(SERVICE, id).and_then(|e| e.get_password()).map_err(|e| e.to_string())
    }

    /// Entrada que não existe não é erro: o objetivo (não ter segredo guardado) já está cumprido.
    pub fn delete(id: &str) -> Result<(), String> {
        match keyring::Entry::new(SERVICE, id).and_then(|e| e.delete_credential()) {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
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

    pub fn delete(id: &str) -> Result<(), String> {
        match entry(id)?.delete_credential() {
            Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

#[cfg(target_os = "android")]
pub use imp::init;
pub use imp::{delete, get, set};
