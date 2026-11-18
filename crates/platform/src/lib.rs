//! Platform capabilities and the degradation chain (spec 29).
//!
//! # The rule this crate enforces
//!
//! Spec 29.1: assume nothing about windowing. Every capability carries one of the five
//! documented states, and only states that represent demonstrated behaviour are used.
//! In particular "unverified" is NOT silently treated as working -- that is how a
//! product ends up promising a popup it cannot draw.
//!
//! # Why the chain lives here and not in the UI
//!
//! The choice "desktop popup, else system notification, else an in-app marker" is a
//! policy with a fallback order. Putting it in one tested function means the UI cannot
//! accidentally skip a step, and the decision can be asserted without a window.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

use serde::{Deserialize, Serialize};

/// The only states a capability may have (spec 29.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityState {
    /// Works on this platform with no extra work.
    Native,
    /// Works with known limitations.
    Partial,
    /// Needs a desktop-environment specific implementation that has not been built.
    DesktopSpecific,
    /// Cannot be done here; the product must degrade to a system notification.
    NotificationFallback,
    /// Never tested on this platform. Treated as unusable.
    Unverified,
}

impl CapabilityState {
    /// Whether a capability in this state may be relied on for presentation.
    ///
    /// Only demonstrated states count. Treating Unverified as usable is precisely the
    /// failure mode spec 29.1 warns about.
    pub fn is_usable(self) -> bool {
        matches!(self, CapabilityState::Native | CapabilityState::Partial)
    }

    /// Short label for the compatibility matrix.
    pub fn label(self) -> &'static str {
        match self {
            CapabilityState::Native => "native",
            CapabilityState::Partial => "partial",
            CapabilityState::DesktopSpecific => "desktop-specific",
            CapabilityState::NotificationFallback => "notification-fallback",
            CapabilityState::Unverified => "unverified",
        }
    }
}

/// Where a notification can be shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    /// The product's own popup window.
    DesktopPopup,
    /// The operating system's notification system.
    SystemNotification,
    /// Nothing can be shown; the gallery's new-unlock marker carries the information.
    InAppOnly,
}

/// What to do when a fullscreen application is running (spec 29.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FullscreenPolicy {
    /// Default: fall back to a system notification.
    #[default]
    DegradeToSystemNotification,
    /// Hold the notification until fullscreen ends.
    Defer,
    /// Stay silent and rely on the in-app marker.
    Silent,
}

/// The outcome of the fullscreen decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FullscreenOutcome {
    /// Present now on the given channel.
    Present(Channel),
    /// Hold it; the caller applies the freshness policy when it finally presents.
    Defer,
}

/// Everything the product needs to know about the host.
///
/// Serialised in camelCase because this struct crosses into the webview, where the
/// TypeScript interface reads `desktopPopup`. It was snake_case on the wire while the
/// UI expected camelCase, so the About page read `undefined.replace(...)` and crashed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    /// A borderless transparent popup window.
    pub desktop_popup: CapabilityState,
    /// The operating system's notification system.
    pub system_notification: CapabilityState,
    /// A system tray / menu bar icon.
    pub tray: CapabilityState,
    /// Detecting the OS do-not-disturb state.
    pub do_not_disturb_detection: CapabilityState,
    /// Choosing which monitor a popup appears on.
    pub multi_monitor: CapabilityState,
    /// Per-monitor DPI awareness.
    pub dpi_scaling: CapabilityState,
    /// Running the shared core as WebAssembly.
    pub wasm: CapabilityState,
}

impl Default for Capabilities {
    fn default() -> Self {
        // The honest default: nothing has been demonstrated yet (spec 29.1, 29.6).
        Capabilities {
            desktop_popup: CapabilityState::Unverified,
            system_notification: CapabilityState::Unverified,
            tray: CapabilityState::Unverified,
            do_not_disturb_detection: CapabilityState::Unverified,
            multi_monitor: CapabilityState::Unverified,
            dpi_scaling: CapabilityState::Unverified,
            wasm: CapabilityState::Unverified,
        }
    }
}

impl Capabilities {
    /// Pick a presentation channel.
    ///
    /// Desktop popup, else the in-app marker. System notifications are deliberately
    /// not part of the chain any more: Windows could not be made to accept them from
    /// this product (ADR-0015), and a channel that depends on the OS accepting an
    /// identity we cannot establish is not a channel. The step is still modelled in
    /// the enum so the compatibility matrix can record where it fails.
    pub fn channel(&self) -> Channel {
        if self.desktop_popup.is_usable() {
            Channel::DesktopPopup
        } else {
            Channel::InAppOnly
        }
    }

    /// Apply the fullscreen policy (spec 29.5).
    ///
    /// The spec's first option is a system notification, and that route is not used
    /// here (ADR-0015). Spec 29.5 already defines the next step for a platform that
    /// cannot show anything: stay silent and let the badge carry it.
    pub fn when_fullscreen(&self, policy: FullscreenPolicy) -> FullscreenOutcome {
        match policy {
            FullscreenPolicy::DegradeToSystemNotification => {
                FullscreenOutcome::Present(Channel::InAppOnly)
            }
            FullscreenPolicy::Defer => FullscreenOutcome::Defer,
            FullscreenPolicy::Silent => FullscreenOutcome::Present(Channel::InAppOnly),
        }
    }

    /// A one-line summary for diagnostics.
    pub fn summary(&self) -> String {
        format!(
            "popup={} notification={} tray={}",
            self.desktop_popup.label(),
            self.system_notification.label(),
            self.tray.label()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caps(popup: CapabilityState, notification: CapabilityState) -> Capabilities {
        Capabilities {
            desktop_popup: popup,
            system_notification: notification,
            ..Capabilities::default()
        }
    }

    #[test]
    fn an_unverified_capability_is_not_treated_as_working() {
        // Spec 29.1: this is the failure mode the rule exists to prevent.
        assert!(!CapabilityState::Unverified.is_usable());
        assert!(!CapabilityState::DesktopSpecific.is_usable());
        assert!(!CapabilityState::NotificationFallback.is_usable());
        assert!(CapabilityState::Native.is_usable());
        assert!(CapabilityState::Partial.is_usable());
    }

    #[test]
    fn a_default_capability_set_claims_nothing() {
        let caps = Capabilities::default();
        assert_eq!(caps.desktop_popup, CapabilityState::Unverified);
        assert_eq!(caps.channel(), Channel::InAppOnly);
    }

    #[test]
    fn the_chain_prefers_the_desktop_popup() {
        assert_eq!(
            caps(CapabilityState::Native, CapabilityState::Native).channel(),
            Channel::DesktopPopup
        );
        assert_eq!(
            caps(CapabilityState::Partial, CapabilityState::Native).channel(),
            Channel::DesktopPopup
        );
    }

    #[test]
    fn the_chain_skips_system_notifications_entirely() {
        // ADR-0015: Windows will not accept toasts from this product, so a capability
        // set that could only notify through the OS must fall straight to the marker
        // rather than promise a channel that does not exist.
        assert_eq!(
            caps(
                CapabilityState::NotificationFallback,
                CapabilityState::Native
            )
            .channel(),
            Channel::InAppOnly
        );
        assert_eq!(
            caps(CapabilityState::Unverified, CapabilityState::Partial).channel(),
            Channel::InAppOnly
        );
    }

    #[test]
    fn the_chain_ends_at_the_in_app_marker() {
        assert_eq!(
            caps(CapabilityState::Unverified, CapabilityState::Unverified).channel(),
            Channel::InAppOnly
        );
    }

    #[test]
    fn fullscreen_degrades_to_the_in_app_marker() {
        // Spec 29.5's first option is a system notification, and that route does not
        // exist here, so the documented further fallback applies.
        let c = caps(CapabilityState::Native, CapabilityState::Native);
        assert_eq!(
            c.when_fullscreen(FullscreenPolicy::DegradeToSystemNotification),
            FullscreenOutcome::Present(Channel::InAppOnly)
        );
    }

    #[test]
    fn fullscreen_without_notifications_degrades_to_the_marker() {
        // Spec 29.5: if the platform cannot show anything, the badge carries it -- the
        // product must not claim a popup it cannot draw.
        let c = caps(CapabilityState::Native, CapabilityState::Unverified);
        assert_eq!(
            c.when_fullscreen(FullscreenPolicy::DegradeToSystemNotification),
            FullscreenOutcome::Present(Channel::InAppOnly)
        );
    }

    #[test]
    fn fullscreen_can_defer() {
        let c = caps(CapabilityState::Native, CapabilityState::Native);
        assert_eq!(
            c.when_fullscreen(FullscreenPolicy::Defer),
            FullscreenOutcome::Defer
        );
    }

    #[test]
    fn fullscreen_can_be_silent() {
        let c = caps(CapabilityState::Native, CapabilityState::Native);
        assert_eq!(
            c.when_fullscreen(FullscreenPolicy::Silent),
            FullscreenOutcome::Present(Channel::InAppOnly)
        );
    }

    #[test]
    fn the_default_fullscreen_policy_is_the_one_the_spec_requires() {
        assert_eq!(
            FullscreenPolicy::default(),
            FullscreenPolicy::DegradeToSystemNotification
        );
    }

    #[test]
    fn the_five_documented_states_are_all_distinct() {
        // Spec 29.1 allows exactly five states; a sixth would mean the matrix and the
        // code disagree about what can be recorded.
        let all = [
            CapabilityState::Native,
            CapabilityState::Partial,
            CapabilityState::DesktopSpecific,
            CapabilityState::NotificationFallback,
            CapabilityState::Unverified,
        ];
        let labels: std::collections::BTreeSet<&str> = all.iter().map(|s| s.label()).collect();
        assert_eq!(labels.len(), 5);
    }

    #[test]
    fn a_capability_set_survives_a_json_round_trip() {
        let c = Capabilities {
            desktop_popup: CapabilityState::NotificationFallback,
            system_notification: CapabilityState::Native,
            tray: CapabilityState::Native,
            do_not_disturb_detection: CapabilityState::Unverified,
            multi_monitor: CapabilityState::Partial,
            dpi_scaling: CapabilityState::Native,
            wasm: CapabilityState::Native,
        };
        let json = serde_json::to_string(&c).expect("serialise");
        let back: Capabilities = serde_json::from_str(&json).expect("deserialise");
        assert_eq!(c, back);
    }

    #[test]
    fn the_summary_names_the_three_presentation_capabilities() {
        let c = caps(
            CapabilityState::NotificationFallback,
            CapabilityState::Unverified,
        );
        let s = c.summary();
        assert!(s.contains("popup=notification-fallback"));
        assert!(s.contains("notification=unverified"));
    }

    #[test]
    fn capabilities_are_camel_case_on_the_wire() {
        // The About page crashed on `undefined.replace(...)` because these names did
        // not match the TypeScript interface. Nothing compared them until this test.
        let json = serde_json::to_value(Capabilities::default()).expect("serialise");
        let mut keys: Vec<String> = json.as_object().expect("object").keys().cloned().collect();
        keys.sort();
        assert_eq!(
            keys,
            vec![
                "desktopPopup",
                "doNotDisturbDetection",
                "dpiScaling",
                "multiMonitor",
                "systemNotification",
                "tray",
                "wasm",
            ]
        );
    }
}
