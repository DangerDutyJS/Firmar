//! Bloqueo de la app con PIN y/o Windows Hello.
//!
//! Protege frente a otra persona que use tu sesión de Windows abierta: mientras
//! la app está bloqueada, el backend rechaza cualquier acceso a las claves y a
//! la bóveda. No sustituye la contraseña de Windows: quien tenga tu sesión de
//! Windows y herramientas técnicas podría leer el llavero del sistema.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use argon2::{Algorithm, Argon2, Params, Version};
use serde::{Deserialize, Serialize};

use crate::crypto;
use crate::error::{FirmarError, FirmarResult};
use crate::storage::atomic_write;

const LOCK_FILE: &str = "lock.json";
const MIN_PIN: usize = 4;
const MAX_PIN: usize = 32;
const FREE_ATTEMPTS: u32 = 5;
const MAX_BACKOFF: Duration = Duration::from_secs(15 * 60);
// Rápido para el usuario (≈0,1 s) pero caro para probar PIN por fuerza bruta.
const PIN_M_COST_KIB: u32 = 19 * 1024;
const PIN_T_COST: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PinHash {
    #[serde(with = "crate::domain::b64")]
    salt: Vec<u8>,
    #[serde(with = "crate::domain::b64")]
    hash: Vec<u8>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LockConfig {
    pin: Option<PinHash>,
    #[serde(default)]
    hello: bool,
    /// Minutos de inactividad antes de bloquear; 0 = solo al abrir la app.
    #[serde(default)]
    auto_lock_minutes: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LockStatus {
    pub enabled: bool,
    pub locked: bool,
    pub hello: bool,
    pub auto_lock_minutes: u32,
    /// Segundos que faltan para poder volver a intentar el PIN.
    pub retry_in_seconds: u64,
}

pub struct AppLock {
    path: PathBuf,
    config: LockConfig,
    unlocked: bool,
    failures: u32,
    blocked_until: Option<Instant>,
}

impl AppLock {
    pub fn load(vault_root: &Path) -> Self {
        let path = vault_root.join(LOCK_FILE);
        let config: LockConfig = fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        let unlocked = config.pin.is_none();
        Self { path, config, unlocked, failures: 0, blocked_until: None }
    }

    pub fn is_locked(&self) -> bool {
        self.config.pin.is_some() && !self.unlocked
    }

    pub fn ensure_unlocked(&self) -> FirmarResult<()> {
        if self.is_locked() { Err(FirmarError::Locked) } else { Ok(()) }
    }

    pub fn status(&self) -> LockStatus {
        LockStatus {
            enabled: self.config.pin.is_some(),
            locked: self.is_locked(),
            hello: self.config.hello,
            auto_lock_minutes: self.config.auto_lock_minutes,
            retry_in_seconds: self.retry_in().as_secs(),
        }
    }

    /// Activa el bloqueo o cambia el PIN. Requiere la app desbloqueada.
    pub fn set_pin(&mut self, pin: &str) -> FirmarResult<()> {
        self.ensure_unlocked()?;
        let len = pin.chars().count();
        if !(MIN_PIN..=MAX_PIN).contains(&len) {
            return Err(FirmarError::InvalidInput(format!("el PIN debe tener entre {MIN_PIN} y {MAX_PIN} caracteres")));
        }
        let salt = crypto::random_bytes::<16>()?.to_vec();
        let hash = hash_pin(pin, &salt)?;
        self.config.pin = Some(PinHash { salt, hash });
        self.unlocked = true;
        self.save()
    }

    pub fn disable(&mut self) -> FirmarResult<()> {
        self.ensure_unlocked()?;
        self.config = LockConfig::default();
        self.unlocked = true;
        self.save()
    }

    pub fn set_options(&mut self, hello: bool, auto_lock_minutes: u32) -> FirmarResult<()> {
        self.ensure_unlocked()?;
        if self.config.pin.is_none() {
            return Err(FirmarError::InvalidInput("primero configura un PIN".into()));
        }
        self.config.hello = hello;
        self.config.auto_lock_minutes = auto_lock_minutes.min(24 * 60);
        self.save()
    }

    pub fn unlock_with_pin(&mut self, pin: &str) -> FirmarResult<()> {
        let Some(stored) = self.config.pin.clone() else {
            self.unlocked = true;
            return Ok(());
        };
        let wait = self.retry_in();
        if !wait.is_zero() {
            return Err(FirmarError::InvalidInput(format!(
                "demasiados intentos; espera {} s",
                wait.as_secs().max(1)
            )));
        }
        if constant_time_eq(&hash_pin(pin, &stored.salt)?, &stored.hash) {
            self.unlocked = true;
            self.failures = 0;
            self.blocked_until = None;
            Ok(())
        } else {
            self.failures += 1;
            if self.failures >= FREE_ATTEMPTS {
                let exp = (self.failures - FREE_ATTEMPTS).min(10);
                let backoff = Duration::from_secs(30 * (1u64 << exp)).min(MAX_BACKOFF);
                self.blocked_until = Some(Instant::now() + backoff);
            }
            Err(FirmarError::InvalidInput("PIN incorrecto".into()))
        }
    }

    /// Desbloqueo tras una verificación correcta de Windows Hello.
    pub fn unlock_verified(&mut self) -> FirmarResult<()> {
        if !self.config.hello {
            return Err(FirmarError::InvalidInput("Windows Hello no está activado para Firmar Vecipp".into()));
        }
        self.unlocked = true;
        self.failures = 0;
        self.blocked_until = None;
        Ok(())
    }

    pub fn lock_now(&mut self) {
        if self.config.pin.is_some() {
            self.unlocked = false;
        }
    }

    /// Tras borrar todos los datos: sin bloqueo.
    pub fn reset(&mut self) {
        self.config = LockConfig::default();
        self.unlocked = true;
        self.failures = 0;
        self.blocked_until = None;
    }

    fn retry_in(&self) -> Duration {
        self.blocked_until.map(|t| t.saturating_duration_since(Instant::now())).unwrap_or_default()
    }

    fn save(&self) -> FirmarResult<()> {
        atomic_write(&self.path, &serde_json::to_vec_pretty(&self.config)?)
    }
}

fn hash_pin(pin: &str, salt: &[u8]) -> FirmarResult<Vec<u8>> {
    let params = Params::new(PIN_M_COST_KIB, PIN_T_COST, 1, Some(32)).map_err(|e| FirmarError::Crypto(e.to_string()))?;
    let mut out = vec![0u8; 32];
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(pin.as_bytes(), salt, &mut out)
        .map_err(|e| FirmarError::Crypto(e.to_string()))?;
    Ok(out)
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pin_lock_lifecycle_and_persistence() {
        let dir = tempfile::tempdir().unwrap();
        let mut lock = AppLock::load(dir.path());
        assert!(!lock.is_locked());
        assert!(lock.set_pin("12").is_err(), "PIN demasiado corto");
        lock.set_pin("2468").unwrap();
        lock.lock_now();
        assert!(matches!(lock.ensure_unlocked(), Err(FirmarError::Locked)));

        // Al reabrir la app empieza bloqueada.
        let mut reopened = AppLock::load(dir.path());
        assert!(reopened.is_locked());
        assert!(reopened.unlock_with_pin("0000").is_err());
        reopened.unlock_with_pin("2468").unwrap();
        assert!(!reopened.is_locked());
        assert!(reopened.unlock_verified().is_err(), "Hello no activado");
    }

    #[test]
    fn repeated_failures_trigger_backoff() {
        let dir = tempfile::tempdir().unwrap();
        let mut lock = AppLock::load(dir.path());
        lock.set_pin("2468").unwrap();
        lock.lock_now();
        for _ in 0..FREE_ATTEMPTS {
            assert!(lock.unlock_with_pin("1111").is_err());
        }
        assert!(lock.status().retry_in_seconds > 0);
        let err = lock.unlock_with_pin("2468").unwrap_err().to_string();
        assert!(err.contains("espera"), "ni el PIN correcto entra durante la espera: {err}");
    }
}
