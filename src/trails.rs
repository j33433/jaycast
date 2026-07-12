use wasm_bindgen::JsValue;
use web_sys::window;

const TRAIL_PREF_KEY: &str = "jaycast:trail";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trail {
    CampMurphy,
    Markham,
    QuietWaters,
}

impl Trail {
    pub const ALL: [Self; 3] = [Self::CampMurphy, Self::Markham, Self::QuietWaters];

    pub fn slug(self) -> &'static str {
        match self {
            Self::CampMurphy => "camp-murphy",
            Self::Markham => "markham",
            Self::QuietWaters => "quiet-waters",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::CampMurphy => "Camp Murphy MTB Trails",
            Self::Markham => "Markham Park Mountain Bike Trails",
            Self::QuietWaters => "Quiet Waters Park Mountain Bike Trails",
        }
    }

    pub fn location(self) -> &'static str {
        match self {
            Self::CampMurphy => "Jonathan Dickinson State Park, FL",
            Self::Markham => "Weston, FL",
            Self::QuietWaters => "Deerfield Beach, FL",
        }
    }

    pub fn latitude(self) -> f64 {
        match self {
            Self::CampMurphy => 27.012_260_963_502_648,
            Self::Markham => 26.129_830_519_474_492,
            Self::QuietWaters => 26.310_122_948_237_12,
        }
    }

    pub fn longitude(self) -> f64 {
        match self {
            Self::CampMurphy => -80.110_818_376_202_99,
            Self::Markham => -80.350_903_575_218_13,
            Self::QuietWaters => -80.161_129_684_606_51,
        }
    }

    pub fn icon_src(self) -> &'static str {
        match self {
            Self::CampMurphy => "/jaycast/jaycast-plain.svg",
            Self::Markham => "/jaycast/gatorcast-plain.svg",
            Self::QuietWaters => "/jaycast/eaglecast-plain.svg",
        }
    }

    pub fn short_name(self) -> &'static str {
        match self {
            Self::CampMurphy => "Camp Murphy",
            Self::Markham => "Markham",
            Self::QuietWaters => "Quiet Waters",
        }
    }

    pub fn tagline(self) -> &'static str {
        match self {
            Self::CampMurphy => "scrub trail pack",
            Self::Markham => "drainage advisory",
            Self::QuietWaters => "mixed-surface forecast",
        }
    }

    pub fn from_slug(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|trail| trail.slug() == value)
    }
}

pub fn load_trail_pref() -> Trail {
    trail_from_url()
        .or_else(|| {
            window()
                .and_then(|w| w.local_storage().ok().flatten())
                .and_then(|storage| storage.get_item(TRAIL_PREF_KEY).ok().flatten())
                .and_then(|slug| Trail::from_slug(&slug))
        })
        .unwrap_or(Trail::CampMurphy)
}

pub fn save_trail_pref(trail: Trail) {
    if let Some(storage) = window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = storage.set_item(TRAIL_PREF_KEY, trail.slug());
    }
}

pub fn update_trail_url(trail: Trail) {
    let Some(window) = window() else {
        return;
    };
    let Ok(history) = window.history() else {
        return;
    };
    let pathname = window.location().pathname().unwrap_or_default();
    let url = format!("{pathname}?trail={}", trail.slug());
    let _ = history.replace_state_with_url(&JsValue::NULL, "", Some(&url));
}

fn trail_from_url() -> Option<Trail> {
    let search = window()?.location().search().ok()?;
    trail_from_query(&search)
}

fn trail_from_query(query: &str) -> Option<Trail> {
    query
        .trim_start_matches('?')
        .split('&')
        .filter_map(|part| part.split_once('='))
        .find_map(|(key, value)| (key == "trail").then_some(value))
        .and_then(Trail::from_slug)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bookmarkable_trail_query() {
        assert_eq!(trail_from_query("?trail=markham"), Some(Trail::Markham));
        assert_eq!(
            trail_from_query("model=ecmwf&trail=quiet-waters"),
            Some(Trail::QuietWaters)
        );
        assert_eq!(trail_from_query("?trail=unknown"), None);
    }
}
