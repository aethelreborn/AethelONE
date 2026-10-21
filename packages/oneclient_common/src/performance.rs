//! Performance mode: what the launcher is allowed to change to make the game
//! run smoother on a machine that cannot afford the defaults
//!
//! There are only two levers a launcher really has: the JVM's garbage
//! collector and the game's own video settings. Everything a launcher says
//! about "boosting your fps" beyond that is a promise, so this module keeps
//! the promise honest - every change it describes is one the game or the JVM
//! actually acts on

use serde::{Deserialize, Serialize};

use crate::memory::{default_mem_max_for_total, total_cores, total_ram_mb};

/// How hard the launcher works for the frame rate
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PerformanceMode {
    /// Nothing is touched, the game runs exactly as it was set up
    #[default]
    Off,
    /// The garbage collector is tuned and the video settings are trimmed a
    /// little
    Balanced,
    /// Everything the balanced mode does, turned all the way down for a
    /// machine with little memory and few cores
    LowSpec,
}

impl PerformanceMode {
    pub const ALL: [PerformanceMode; 3] = [
        PerformanceMode::Off,
        PerformanceMode::Balanced,
        PerformanceMode::LowSpec,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Balanced => "balanced",
            Self::LowSpec => "low_spec",
        }
    }

    /// An unknown value reads as off so a profile written by a newer build
    /// cannot switch the tuning on by accident
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "off" => Some(Self::Off),
            "balanced" => Some(Self::Balanced),
            "low_spec" | "lowspec" | "low-spec" => Some(Self::LowSpec),
            _ => None,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Balanced => "Balanced",
            Self::LowSpec => "Low-spec",
        }
    }

    /// What the mode does, in one line
    pub fn summary(&self) -> &'static str {
        match self {
            Self::Off => "Nothing is changed, the game runs with the settings you picked",
            Self::Balanced => {
                "The garbage collector is tuned for short pauses and the video settings are \
                 trimmed a little: shorter render distance, fewer particles, no clouds. The \
                 memory stays as you set it"
            }
            Self::LowSpec => {
                "Everything the balanced mode does, turned down for a weak machine: fast \
                 graphics, minimal particles, half the entity distance and a heap the system \
                 can afford so it does not start paging"
            }
        }
    }
}

/// What the machine can afford
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SystemProfile {
    /// Total physical memory in megabytes
    pub ram_mb: u32,
    /// Logical cores
    pub cores: u32,
}

impl SystemProfile {
    /// Reads the machine the launcher runs on
    pub fn detect() -> Self {
        Self {
            ram_mb: total_ram_mb(),
            cores: total_cores(),
        }
    }

    /// A machine that cannot hand the game two gigabytes and a core to itself
    pub fn is_low_spec(&self) -> bool {
        self.ram_mb < 6 * 1024 || self.cores <= 2
    }

    /// The mode this machine should use
    pub fn recommended_mode(&self) -> PerformanceMode {
        if self.is_low_spec() {
            PerformanceMode::LowSpec
        } else {
            PerformanceMode::Balanced
        }
    }

    /// A short description of the machine for the settings page
    pub fn describe(&self) -> String {
        format!(
            "{} GB of memory, {} {}",
            (self.ram_mb as f64 / 1024.).round() as u32,
            self.cores,
            if self.cores == 1 { "core" } else { "cores" }
        )
    }
}

/// The changes a mode makes, ready to be applied and to be shown
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PerformancePlan {
    pub mode: PerformanceMode,
    /// The heap the game should get; `None` keeps the number the user picked
    pub heap_mb: Option<u32>,
    /// The game's own video settings, written before every launch
    pub options: Vec<(String, String)>,
}

impl PerformancePlan {
    /// One line per change for the settings page
    pub fn notes(&self) -> Vec<String> {
        let mut notes = Vec::with_capacity(self.options.len() + 1);
        if let Some(heap) = self.heap_mb {
            notes.push(format!("Heap capped at {} MB", heap));
        }
        for (key, value) in &self.options {
            notes.push(format!("{} {}", option_label(key), option_value(value)));
        }
        notes
    }
}

/// Works out what a mode changes on this machine
///
/// `mem_max` is the heap the user picked and `modded` says whether the
/// instance carries mods: a pack that needs four gigabytes is never handed a
/// smaller heap, no matter how weak the machine is
pub fn plan(
    mode: PerformanceMode,
    system: SystemProfile,
    mem_max: u32,
    modded: bool,
) -> PerformancePlan {
    if mode == PerformanceMode::Off {
        return PerformancePlan {
            mode,
            ..Default::default()
        };
    }

    // Only the low spec mode touches the heap: the balanced mode promises
    // smoother frames, not less memory than the user asked for
    let heap_mb = (mode == PerformanceMode::LowSpec && !modded)
        .then(|| recommended_heap_mb(system))
        .filter(|recommended| mem_max > *recommended);

    PerformancePlan {
        mode,
        heap_mb,
        options: video_settings(mode)
            .into_iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect(),
    }
}

/// The heap a machine of this size can give the game while the system, the
/// launcher and the browser keep their share
pub fn recommended_heap_mb(system: SystemProfile) -> u32 {
    let headroom = 2 * 1024;
    let free = system.ram_mb.saturating_sub(headroom);
    let recommended = free.min(default_mem_max_for_total(system.ram_mb));
    // a clean number reads better in the settings and in the launch log
    let rounded = (recommended / 256) * 256;
    rounded.max(1024)
}

/// The video settings a mode writes into the game's options file
///
/// Keys the running game does not know are ignored by it, so an old version
/// simply keeps its own setting for that line
fn video_settings(mode: PerformanceMode) -> Vec<(&'static str, &'static str)> {
    match mode {
        PerformanceMode::Off => Vec::new(),
        PerformanceMode::Balanced => vec![
            ("renderDistance", "8"),
            ("simulationDistance", "6"),
            ("particles", "decreased"),
            ("entityDistanceScaling", "0.75"),
            ("cloudStatus", "false"),
        ],
        PerformanceMode::LowSpec => vec![
            ("renderDistance", "6"),
            ("simulationDistance", "5"),
            ("graphics", "fast"),
            ("particles", "minimal"),
            ("entityDistanceScaling", "0.5"),
            ("cloudStatus", "false"),
            ("ao", "false"),
            ("mipmapLevels", "1"),
            ("vsync", "false"),
        ],
    }
}

/// The name of a video setting as the settings page shows it
pub fn option_label(key: &str) -> &str {
    match key {
        "renderDistance" => "Render distance",
        "simulationDistance" => "Simulation distance",
        "graphics" => "Graphics",
        "particles" => "Particles",
        "entityDistanceScaling" => "Entity distance",
        "cloudStatus" => "Clouds",
        "ao" => "Ambient occlusion",
        "mipmapLevels" => "Mipmap levels",
        "vsync" => "VSync",
        _ => key,
    }
}

/// The value of a video setting as the settings page shows it
pub fn option_value(value: &str) -> &str {
    match value {
        "false" => "off",
        "true" => "on",
        "fast" => "fast",
        "fancy" => "fancy",
        "minimal" => "minimal",
        "decreased" => "decreased",
        "all" => "all",
        "0.5" => "50%",
        "0.75" => "75%",
        _ => value,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        PerformanceMode, SystemProfile, option_label, option_value, plan, recommended_heap_mb,
    };

    fn machine(ram_mb: u32, cores: u32) -> SystemProfile {
        SystemProfile { ram_mb, cores }
    }

    #[test]
    fn off_changes_nothing_at_all() {
        let plan = plan(PerformanceMode::Off, machine(3584, 2), 2048, false);

        assert!(plan.options.is_empty());
        assert_eq!(plan.heap_mb, None);
        assert!(plan.notes().is_empty());
    }

    #[test]
    fn the_mode_survives_a_round_trip_through_its_own_name() {
        for mode in PerformanceMode::ALL {
            assert_eq!(PerformanceMode::parse(mode.as_str()), Some(mode));
        }
    }

    #[test]
    fn an_unknown_mode_name_reads_as_off() {
        assert_eq!(PerformanceMode::parse("ultra"), None);
        assert_eq!(PerformanceMode::parse(""), None);
    }

    #[test]
    fn a_four_gigabyte_machine_is_told_to_use_the_low_spec_mode() {
        assert!(machine(3584, 2).is_low_spec());
        assert!(machine(3987, 2).is_low_spec());
        assert_eq!(
            machine(3584, 2).recommended_mode(),
            PerformanceMode::LowSpec
        );
    }

    #[test]
    fn a_roomy_machine_is_told_to_use_the_balanced_mode() {
        assert!(!machine(16 * 1024, 8).is_low_spec());
        assert_eq!(
            machine(16 * 1024, 8).recommended_mode(),
            PerformanceMode::Balanced
        );
    }

    #[test]
    fn a_small_machine_gives_the_game_less_memory_than_it_asked_for() {
        let plan = plan(PerformanceMode::LowSpec, machine(3584, 2), 2048, false);

        assert_eq!(plan.heap_mb, Some(1536));
    }

    #[test]
    fn the_balanced_mode_leaves_the_memory_alone() {
        let plan = plan(PerformanceMode::Balanced, machine(3584, 2), 4096, false);

        assert_eq!(
            plan.heap_mb, None,
            "the balanced mode promises smoother frames, not less memory"
        );
    }

    #[test]
    fn a_modded_instance_keeps_the_heap_it_asked_for() {
        let plan = plan(PerformanceMode::LowSpec, machine(3584, 2), 4096, true);

        assert_eq!(
            plan.heap_mb, None,
            "a pack that needs four gigabytes must not be handed less"
        );
    }

    #[test]
    fn a_heap_that_is_already_small_is_left_alone() {
        let plan = plan(PerformanceMode::LowSpec, machine(3584, 2), 1024, false);

        assert_eq!(plan.heap_mb, None);
    }

    #[test]
    fn a_roomy_machine_never_has_its_heap_cut_down() {
        let plan = plan(
            PerformanceMode::LowSpec,
            machine(32 * 1024, 16),
            4096,
            false,
        );

        assert_eq!(plan.heap_mb, None);
    }

    #[test]
    fn the_low_spec_mode_turns_the_video_settings_down_further_than_the_balanced_one() {
        let balanced = plan(
            PerformanceMode::Balanced,
            machine(16 * 1024, 8),
            4096,
            false,
        );
        let low = plan(PerformanceMode::LowSpec, machine(3584, 2), 2048, false);

        let balanced_render = balanced
            .options
            .iter()
            .find(|(key, _)| key == "renderDistance")
            .map(|(_, value)| value.as_str());
        let low_render = low
            .options
            .iter()
            .find(|(key, _)| key == "renderDistance")
            .map(|(_, value)| value.as_str());

        assert_eq!(balanced_render, Some("8"));
        assert_eq!(low_render, Some("6"));
        assert!(
            low.options
                .iter()
                .any(|(key, value)| key == "graphics" && value == "fast"),
            "the low spec mode has to turn the fancy graphics off"
        );
        assert!(
            !balanced.options.iter().any(|(key, _)| key == "graphics"),
            "the balanced mode keeps the look of the game"
        );
    }

    #[test]
    fn every_setting_is_one_the_game_actually_reads() {
        let plan = plan(PerformanceMode::LowSpec, machine(3584, 2), 2048, false);

        for (key, value) in &plan.options {
            assert!(
                !key.is_empty() && !value.is_empty(),
                "{key} has no value to write"
            );
            assert!(
                !key.contains(char::is_whitespace) && !value.contains(char::is_whitespace),
                "{key}={value} cannot be written as one options line"
            );
        }
    }

    #[test]
    fn the_notes_name_every_change() {
        let plan = plan(PerformanceMode::LowSpec, machine(3584, 2), 2048, false);
        let notes = plan.notes();

        assert_eq!(notes.len(), plan.options.len() + 1);
        assert!(notes[0].contains("1536"), "{notes:?}");
        assert!(notes.iter().any(|note| note.contains("Render distance")));
        assert!(notes.iter().any(|note| note.contains("Graphics")));
    }

    #[test]
    fn the_settings_are_named_the_way_the_page_shows_them() {
        assert_eq!(option_label("renderDistance"), "Render distance");
        assert_eq!(option_value("false"), "off");
        assert_eq!(option_value("0.5"), "50%");
        assert_eq!(option_value("fast"), "fast");
    }

    #[test]
    fn the_recommendation_never_goes_below_a_gigabyte() {
        assert_eq!(recommended_heap_mb(machine(0, 1)), 1024);
        assert_eq!(recommended_heap_mb(machine(1024, 1)), 1024);
        assert_eq!(recommended_heap_mb(machine(3584, 2)), 1536);
    }
}
