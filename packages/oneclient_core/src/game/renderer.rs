//! What the game is drawing with, read from the log the last launch left
//!
//! A machine with no working graphics driver draws the whole game on the
//! processor, and then no setting in the launcher can make it fast: the
//! frames are made by the same processor that runs the game. The mod can see
//! this while the game runs, the launcher has to read it out of the log the
//! last launch left - which is the only chance to say it before the player
//! spends an evening with two frames per second

/// The renderers that draw on the processor
const SOFTWARE_RENDERERS: [&str; 7] = [
    "llvmpipe",
    "lavapipe",
    "softpipe",
    "swiftshader",
    "microsoft basic render driver",
    "gdi generic",
    "mesa/x.org",
];

/// The renderer the game drew with
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Renderer {
    /// The name the log gave, the way the user would recognise it
    pub name: String,
    /// Whether the frames are made by the processor
    pub software: bool,
}

impl Renderer {
    /// What to tell the user when the game is drawn by the processor
    #[must_use]
    pub fn software_body(&self) -> String {
        format!(
            "The last launch drew the game with {}, which runs on the processor instead of a \
             graphics card. The game then shares the processor with the frames, and no setting \
             in the launcher can fix that - it needs a working graphics driver. On a laptop the \
             driver usually comes from the maker of the machine.",
            self.name
        )
    }
}

/// Reads the renderer out of a game log
///
/// The game writes the name of the renderer it got once it starts, older
/// versions write `GL_RENDERER: name` and the newer ones `OpenGL renderer:
/// name`
#[must_use]
pub fn renderer_from_log(log: &str) -> Option<Renderer> {
    log.lines().find_map(renderer_in)
}

fn renderer_in(line: &str) -> Option<Renderer> {
    let name = line
        .split_once("GL_RENDERER: ")
        .or_else(|| line.split_once("OpenGL renderer: "))
        .map(|(_, name)| name.trim())?
        .trim_end_matches(['\r', '\n']);

    if name.is_empty() {
        return None;
    }

    let lower = name.to_lowercase();
    let software = SOFTWARE_RENDERERS
        .iter()
        .any(|marker| lower.contains(marker));

    Some(Renderer {
        name: name.to_string(),
        software,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_renderer_is_read_out_of_the_old_log_line() {
        let log = "[Render thread/INFO]: OpenGL: GL_RENDERER: AMD Radeon(TM) Graphics\n";

        let renderer = renderer_from_log(log).unwrap();

        assert_eq!(renderer.name, "AMD Radeon(TM) Graphics");
        assert!(!renderer.software);
    }

    #[test]
    fn the_renderer_is_read_out_of_the_new_log_line() {
        let log = "[Render thread/INFO]: OpenGL renderer: NVIDIA GeForce RTX 3060\n";

        let renderer = renderer_from_log(log).unwrap();

        assert_eq!(renderer.name, "NVIDIA GeForce RTX 3060");
        assert!(!renderer.software);
    }

    #[test]
    fn a_software_renderer_is_named_as_one() {
        for name in [
            "llvmpipe (LLVM 15.0.7, 256 bits)",
            "ANGLE (Microsoft, Microsoft Basic Render Driver Direct3D11 vs_5_0 ps_5_0, D3D11)",
            "Google SwiftShader",
            "lavapipe",
        ] {
            let log = format!("[Render thread/INFO]: OpenGL: GL_RENDERER: {name}\n");

            let renderer = renderer_from_log(&log).unwrap();

            assert!(renderer.software, "{name} draws on the processor");
        }
    }

    #[test]
    fn a_real_card_is_not_called_a_software_renderer() {
        for name in [
            "AMD Radeon(TM) Graphics",
            "Intel(R) Iris(R) Xe Graphics",
            "Mesa Intel(R) UHD Graphics (CML GT2)",
        ] {
            let log = format!("[Render thread/INFO]: OpenGL: GL_RENDERER: {name}\n");

            let renderer = renderer_from_log(&log).unwrap();

            assert!(!renderer.software, "{name} is a graphics card");
        }
    }

    #[test]
    fn a_log_without_a_renderer_says_nothing() {
        assert!(renderer_from_log("").is_none());
        assert!(renderer_from_log("[main/INFO]: Starting minecraft server").is_none());
        assert!(renderer_from_log("[Render thread/INFO]: OpenGL: GL_RENDERER: ").is_none());
    }

    #[test]
    fn the_first_renderer_in_the_log_is_the_one_that_matters() {
        let log = "[Render thread/INFO]: OpenGL: GL_RENDERER: llvmpipe (LLVM 15.0.7)\n\
                   [Render thread/INFO]: OpenGL: GL_RENDERER: something else\n";

        let renderer = renderer_from_log(log).unwrap();

        assert_eq!(renderer.name, "llvmpipe (LLVM 15.0.7)");
    }

    #[test]
    fn the_advice_names_the_renderer_and_the_driver() {
        let renderer = Renderer {
            name: "llvmpipe (LLVM 15.0.7)".to_string(),
            software: true,
        };

        let body = renderer.software_body();

        assert!(body.contains("llvmpipe"), "{body}");
        assert!(body.contains("driver"), "{body}");
    }
}
