//! Application identity: dev, production, and sandbox profiles never share state.

use std::path::PathBuf;

/// Application branding metadata so any app built on the framework can isolate
/// its own XDG directories, IPC sockets, and environment variable overrides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppBranding {
    pub prod_app_id: &'static str,
    pub dev_app_id: &'static str,
    pub sandbox_app_id: &'static str,
    pub prod_dir_name: &'static str,
    pub dev_dir_name: &'static str,
    pub sandbox_dir_name: &'static str,
    pub prod_display_name: &'static str,
    pub dev_display_name: &'static str,
    pub sandbox_display_name: &'static str,
    pub env_prefix: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Profile {
    Dev,
    Prod,
    Sandbox(PathBuf),
}

impl Profile {
    /// Debug builds default to `Dev`; release builds default to `Prod`.
    /// `<ENV_PREFIX>_PROFILE=dev|prod` overrides in debug builds, and
    /// `<ENV_PREFIX>_SANDBOX_DIR` selects an isolated `Sandbox` root.
    pub fn current_for(env_prefix: &str) -> Self {
        let sandbox_var = format!("{env_prefix}_SANDBOX_DIR");
        if let Ok(dir) = std::env::var(&sandbox_var)
            && !dir.trim().is_empty()
        {
            return Self::Sandbox(PathBuf::from(dir));
        }

        let profile_var = format!("{env_prefix}_PROFILE");
        if cfg!(debug_assertions) {
            if let Ok(value) = std::env::var(&profile_var) {
                match value.trim().to_ascii_lowercase().as_str() {
                    "dev" => return Self::Dev,
                    "prod" => return Self::Prod,
                    _ => {}
                }
            }
            Self::Dev
        } else if let Ok(value) = std::env::var(&profile_var) {
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

    pub const fn app_id_for(&self, branding: &AppBranding) -> &'static str {
        match self {
            Self::Dev => branding.dev_app_id,
            Self::Prod => branding.prod_app_id,
            Self::Sandbox(_) => branding.sandbox_app_id,
        }
    }

    pub const fn dir_name_for(&self, branding: &AppBranding) -> &'static str {
        match self {
            Self::Dev => branding.dev_dir_name,
            Self::Prod => branding.prod_dir_name,
            Self::Sandbox(_) => branding.sandbox_dir_name,
        }
    }

    pub const fn display_name_for(&self, branding: &AppBranding) -> &'static str {
        match self {
            Self::Dev => branding.dev_display_name,
            Self::Prod => branding.prod_display_name,
            Self::Sandbox(_) => branding.sandbox_display_name,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppIdentity {
    pub profile: Profile,
    pub branding: AppBranding,
    pub app_id: &'static str,
    pub display_name: &'static str,
    pub version: &'static str,
    pub protocol_version: u32,
}

impl AppIdentity {
    pub fn current_for(branding: AppBranding, protocol_version: u32) -> Self {
        let profile = Profile::current_for(branding.env_prefix);
        Self::for_profile_with(profile, branding, protocol_version)
    }

    pub fn for_profile_with(
        profile: Profile,
        branding: AppBranding,
        protocol_version: u32,
    ) -> Self {
        Self {
            app_id: profile.app_id_for(&branding),
            display_name: profile.display_name_for(&branding),
            profile,
            branding,
            version: env!("CARGO_PKG_VERSION"),
            protocol_version,
        }
    }
}
