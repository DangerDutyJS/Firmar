//! Windows Hello (huella, rostro o PIN de Windows) mediante
//! `UserConsentVerifier`. En otros sistemas no está disponible.

use crate::error::FirmarResult;

#[cfg(windows)]
mod imp {
    use windows::Security::Credentials::UI::{
        UserConsentVerificationResult, UserConsentVerifier, UserConsentVerifierAvailability,
    };
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::WinRT::IUserConsentVerifierInterop;
    use windows::core::{HSTRING, factory};
    use windows_future::IAsyncOperation;

    use crate::error::{FirmarError, FirmarResult};

    pub fn available() -> bool {
        UserConsentVerifier::CheckAvailabilityAsync()
            .and_then(|op| op.join())
            .map(|a| a == UserConsentVerifierAvailability::Available)
            .unwrap_or(false)
    }

    /// Muestra el diálogo de Windows Hello sobre la ventana `hwnd`.
    pub fn verify(hwnd: isize, message: &str) -> FirmarResult<bool> {
        let err = |e: windows::core::Error| FirmarError::Crypto(format!("Windows Hello: {}", e.message()));
        let interop = factory::<UserConsentVerifier, IUserConsentVerifierInterop>().map_err(err)?;
        // SAFETY: `hwnd` es el identificador de la ventana principal de la app,
        // válido mientras dura la llamada.
        let op: IAsyncOperation<UserConsentVerificationResult> =
            unsafe { interop.RequestVerificationForWindowAsync(HWND(hwnd as *mut _), &HSTRING::from(message)) }.map_err(err)?;
        Ok(op.join().map_err(err)? == UserConsentVerificationResult::Verified)
    }
}

#[cfg(not(windows))]
mod imp {
    use crate::error::{FirmarError, FirmarResult};

    pub fn available() -> bool {
        false
    }

    pub fn verify(_hwnd: isize, _message: &str) -> FirmarResult<bool> {
        Err(FirmarError::UnsupportedFormat("Windows Hello solo está disponible en Windows".into()))
    }
}

pub fn available() -> bool {
    imp::available()
}

pub fn verify(hwnd: isize, message: &str) -> FirmarResult<bool> {
    imp::verify(hwnd, message)
}
