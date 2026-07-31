//! Rain gauge station specs — single source of truth for both WASM and CLI.

use crate::trails::Trail;

#[derive(Clone, Copy, Debug)]
pub struct GaugeSpec {
    pub id: &'static str,
    pub role: &'static str,
    pub lat: f64,
    pub lon: f64,
}

pub fn stations_for_trail(trail: Trail) -> &'static [GaugeSpec] {
    match trail {
        Trail::Markham => &[
            GaugeSpec {
                id: "MID_E8181",
                role: "primary",
                lat: 26.121_17,
                lon: -80.409_33,
            },
            GaugeSpec {
                id: "PWS_W4RCT",
                role: "secondary",
                lat: 26.106_63,
                lon: -80.361_71,
            },
        ],
        Trail::CampMurphy => &[
            GaugeSpec {
                id: "MID_C8019",
                role: "primary",
                lat: 26.967_510,
                lon: -80.097_351,
            },
            GaugeSpec {
                id: "PWS_JOE4SPEED",
                role: "primary",
                lat: 26.967_62,
                lon: -80.097_37,
            },
            GaugeSpec {
                id: "PWS_LPWS9943",
                role: "secondary",
                lat: 27.082_75,
                lon: -80.137_07,
            },
        ],
        Trail::QuietWaters => &[
            GaugeSpec {
                id: "PWS_363636363",
                role: "primary",
                lat: 26.344_482,
                lon: -80.163_688,
            },
            GaugeSpec {
                id: "MID_C6162",
                role: "secondary",
                lat: 26.310_830,
                lon: -80.174_420,
            },
        ],
    }
}
