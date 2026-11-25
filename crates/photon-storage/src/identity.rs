//! Application identity: dev, production, and sandbox profiles never share state.

use std::path::PathBuf;

pub const PROD_APP_ID: &str = "io.github.tienv2i.Photon";
pub const DEV_APP_ID: &str = "io.github.tienv2i.Photon.Dev";
pub const SANDBOX_APP_ID: &str = "io.github.tienv2i.Photon.Sandbox";
pub const BASE_DISPLAY_NAME: &str = "Photon";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Profile {
    Dev,
    Prod,
    Sandbox(PathBuf),
}

impl Profile {
    /// Debug builds default to `Dev`; release builds default to `Prod`.
    /// `PHOTON_PROFILE=dev|prod` overrides in debug builds, and `PHOTON_SANDBOX_DIR`
    /// selects an isolated `Sandbox` root.
    pub fn current() -> Self {
        if let Ok(dir) = std::env::var("PHOTON_SANDBOX_DIR")
            && !dir.trim().is_empty()
        {
            return Self::Sandbox(PathBuf::from(dir));
        }

        if cfg!(debug_assertions) {
            if let Ok(value) = std::env::var("PHOTON_PROFILE") {
                match value.trim().to_ascii_lowercase().as_str() {
                    "dev" => return Self::Dev,
                    "prod" => return Self::Prod,
                    _ => {}
                }
            }
            Self::Dev
        } else if let Ok(value) = std::env::var("PHOTON_PROFILE") {
            if value.trim().eq_ignore_ascii_case("dev") {
                return Self::Dev;
            }
            Self::Prod
        } else {
            Self::Prod
        }
    }

    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Dev => "dev",
            Self::Prod => "prod",
            Self::Sandbox(_) => "sandbox",
        }
    }

    pub const fn app_id(&self) -> &'static str {
        match self {
            Self::Dev => DEV_APP_ID,
            Self::Prod => PROD_APP_ID,
            Self::Sandbox(_) => SANDBOX_APP_ID,
        }
    }

    pub const fn dir_name(&self) -> &'static str {
        match self {
            Self::Dev => "photon-dev",
            Self::Prod => "photon",
            Self::Sandbox(_) => "photon-sandbox",
        }
    }

    pub const fn display_name(&self) -> &'static str {
        match self {
            Self::Dev => "Photon (Dev)",
            Self::Prod => BASE_DISPLAY_NAME,
            Self::Sandbox(_) => "Photon (Sandbox)",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppIdentity {
    pub profile: Profile,
    pub app_id: &'static str,
    pub display_name: &'static str,
    pub version: &'static str,
    pub protocol_version: u32,
}

impl AppIdentity {
    pub fn current() -> Self {
        Self::for_profile(Profile::current())
    }

    pub fn for_profile(profile: Profile) -> Self {
        Self {
            app_id: profile.app_id(),
            display_name: profile.display_name(),
            profile,
            version: env!("CARGO_PKG_VERSION"),
            protocol_version: photon_protocol::PROTOCOL_VERSION,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dev_and_prod_identities_are_distinct() {
        assert_ne!(DEV_APP_ID, PROD_APP_ID);
        assert_ne!(Profile::Dev.dir_name(), Profile::Prod.dir_name());
        assert_ne!(Profile::Dev.display_name(), Profile::Prod.display_name());
        assert!(DEV_APP_ID.starts_with(PROD_APP_ID));
    }
}
