//! Photon application identity built atop `fw-storage`.

pub use fw_storage::{AppBranding, AppIdentity, Profile};

pub const PROD_APP_ID: &str = "io.github.tienv2i.Photon";
pub const DEV_APP_ID: &str = "io.github.tienv2i.Photon.Dev";
pub const SANDBOX_APP_ID: &str = "io.github.tienv2i.Photon.Sandbox";
pub const BASE_DISPLAY_NAME: &str = "Photon";

pub const PHOTON_BRANDING: AppBranding = AppBranding {
    prod_app_id: PROD_APP_ID,
    dev_app_id: DEV_APP_ID,
    sandbox_app_id: SANDBOX_APP_ID,
    prod_dir_name: "photon",
    dev_dir_name: "photon-dev",
    sandbox_dir_name: "photon-sandbox",
    prod_display_name: BASE_DISPLAY_NAME,
    dev_display_name: "Photon (Dev)",
    sandbox_display_name: "Photon (Sandbox)",
    env_prefix: "PHOTON",
};

pub fn current_identity() -> AppIdentity {
    AppIdentity::current_for(PHOTON_BRANDING, photon_protocol::PROTOCOL_VERSION)
}

pub fn identity_for_profile(profile: Profile) -> AppIdentity {
    AppIdentity::for_profile_with(profile, PHOTON_BRANDING, photon_protocol::PROTOCOL_VERSION)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dev_and_prod_identities_are_distinct() {
        assert_ne!(DEV_APP_ID, PROD_APP_ID);
        assert_ne!(
            Profile::Dev.dir_name_for(&PHOTON_BRANDING),
            Profile::Prod.dir_name_for(&PHOTON_BRANDING)
        );
        assert_ne!(
            Profile::Dev.display_name_for(&PHOTON_BRANDING),
            Profile::Prod.display_name_for(&PHOTON_BRANDING)
        );
        assert!(DEV_APP_ID.starts_with(PROD_APP_ID));
    }
}
