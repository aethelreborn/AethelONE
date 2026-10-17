//! Crash reports name the culprit far more often than the exit code does
//! Minecraft writes one per crash and the Forge family even lists suspects
//! this module turns that text into something a user can act on without
//! reading a stack trace

use std::collections::HashSet;

/// An enabled mod that a crash report may blame
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrashCandidate {
    /// What the user sees in the mods list
    pub label: String,
    /// Lowercase words the report's class names and messages tend to contain
    pub tokens: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CrashTriage {
    /// The report's own "Description" line which is usually the most human
    /// sentence in the whole file
    pub description: Option<String>,
    /// Gauged in order of how sure the evidence is explicit suspects first
    pub suspects: Vec<String>,
}

const MAX_SUSPECTS: usize = 3;

/// Words that appear in half of every classpath and would blame the wrong
/// thing on nearly every crash
const GENERIC_TOKENS: [&str; 15] = [
    "fabric",
    "forge",
    "neoforge",
    "minecraft",
    "client",
    "server",
    "common",
    "config",
    "latest",
    "release",
    "mods",
    "java",
    "core",
    "shared",
    "libs",
];

/// Builds candidate tokens from a mod's display name and file name words
/// shorter than four characters and version-ish digit runs carry no signal
#[must_use]
pub fn candidate_tokens(display_name: Option<&str>, file_name: &str) -> Vec<String> {
    let stem = file_name
        .strip_suffix(".jar")
        .or_else(|| file_name.strip_suffix(".JAR"))
        .unwrap_or(file_name);

    let mut tokens = Vec::new();

    for source in display_name.into_iter().chain(std::iter::once(stem)) {
        for token in source
            .split(|c: char| !c.is_ascii_alphanumeric())
            .map(str::to_ascii_lowercase)
        {
            if token.len() < 4
                || GENERIC_TOKENS.contains(&token.as_str())
                || token.chars().all(|c| c.is_ascii_digit())
                || tokens.contains(&token)
            {
                continue;
            }
            tokens.push(token);
        }
    }

    tokens.truncate(6);
    tokens
}

/// Explicit suspect sections win over stack frames they exist precisely
/// because the loader already worked out who was at fault
#[must_use]
pub fn triage(text: &str, candidates: &[CrashCandidate]) -> CrashTriage {
    let description = description_in(text);

    let explicit = explicit_suspects(text);
    if !explicit.is_empty() {
        return CrashTriage {
            description,
            suspects: explicit,
        };
    }

    CrashTriage {
        description,
        suspects: frame_suspects(text, candidates),
    }
}

fn description_in(text: &str) -> Option<String> {
    for line in text.lines() {
        let line = line.trim();
        for prefix in ["Description:", "Failure message:"] {
            if let Some(rest) = line.strip_prefix(prefix) {
                let rest = rest.trim();
                if !rest.is_empty() {
                    return Some(rest.to_string());
                }
            }
        }
    }

    None
}

/// "Suspected Mod: Sodium (sodium)" from NeoForge and the older comma list
/// "Suspected Mods: Embeddium, Oculus" both land here "None" is a real value
/// in clean reports and must not become a suspect
fn explicit_suspects(text: &str) -> Vec<String> {
    let mut suspects: Vec<String> = Vec::new();

    for line in text.lines() {
        let Some(idx) = line.find("Suspected Mod") else {
            continue;
        };
        let Some((_, rest)) = line[idx..].split_once(':') else {
            continue;
        };

        for part in rest.split(',') {
            let label = name_before_id(part.trim());
            if label.is_empty()
                || label.eq_ignore_ascii_case("none")
                || suspects.iter().any(|s| s.eq_ignore_ascii_case(&label))
            {
                continue;
            }
            suspects.push(label);
            if suspects.len() == MAX_SUSPECTS {
                return suspects;
            }
        }
    }

    suspects
}

/// "Sodium Extra (sodiumextra)" and "sodiumextra" both reduce to something a
/// user recognises
fn name_before_id(part: &str) -> String {
    let name = part.split('(').next().unwrap_or(part).trim();

    if name.is_empty() {
        part.trim_matches(['(', ')']).trim().to_string()
    } else {
        name.to_string()
    }
}

/// Stack frames and jar mentions are the evidence that works on every loader
/// Fabric has no suspect section at all so the first frames are all there is
fn frame_suspects(text: &str, candidates: &[CrashCandidate]) -> Vec<String> {
    let mut matched: Vec<&CrashCandidate> = Vec::new();

    for line in text.lines() {
        let frame = line.trim_start();

        // The Description is prose by mod authors and regularly names sibling
        // mods ("install X alongside Y") only blame it when it is the only
        // thing we have no rather than offer a wrong suspect keep frames strict
        let is_frame = frame.starts_with("at ") || frame.contains(".jar");

        if !is_frame {
            continue;
        }

        let words: HashSet<String> = frame
            .split(|c: char| !c.is_ascii_alphanumeric())
            .map(str::to_ascii_lowercase)
            .collect();

        for candidate in candidates {
            if matched.contains(&candidate) {
                continue;
            }
            if candidate.tokens.iter().any(|token| words.contains(token)) {
                matched.push(candidate);
                if matched.len() == MAX_SUSPECTS {
                    return matched.into_iter().map(|c| c.label.clone()).collect();
                }
            }
        }
    }

    matched.into_iter().map(|c| c.label.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(label: &str, tokens: &[&str]) -> CrashCandidate {
        CrashCandidate {
            label: label.to_string(),
            tokens: tokens.iter().map(|t| t.to_string()).collect(),
        }
    }

    #[test]
    fn a_fabric_stack_frame_blames_the_mod() {
        let text = "\
---- Minecraft Crash Report ----
Description: Rendering overlay

java.lang.NullPointerException: Cannot invoke ...
\tat net.caffeinemc.mods.sodium.client.SodiumClientMod.onInitializeClient(SodiumClientMod.java:55)
\tat net.fabricmc.loader.impl.game.minecraft.Hooks.startClient(Hooks.java:52)";

        let triage = triage(
            text,
            &[
                candidate("Sodium", &["sodium"]),
                candidate("Lithium", &["lithium"]),
            ],
        );

        assert_eq!(triage.description.as_deref(), Some("Rendering overlay"));
        assert_eq!(triage.suspects, vec!["Sodium"]);
    }

    #[test]
    fn an_explicit_suspected_mod_wins_over_frames() {
        let text = "\
---- Minecraft Crash Report ----
Description: Ticking block entity
Suspected Mod: Sodium Extra (sodiumextra)
\tat net.caffeinemc.mods.sodiumextra.SodiumExtra.onTick(SodiumExtra.java:10)";

        let triage = triage(text, &[candidate("Lithium", &["lithium"])]);

        assert_eq!(triage.suspects, vec!["Sodium Extra"]);
    }

    #[test]
    fn the_older_comma_list_is_read_and_none_is_not_a_suspect() {
        let text = "Suspected Mods: Embeddium, Oculus";
        let result = triage(text, &[]);
        assert_eq!(result.suspects, vec!["Embeddium", "Oculus"]);

        let clean = triage("Suspected Mods: None", &[]);
        assert!(clean.suspects.is_empty());
    }

    #[test]
    fn generic_classpath_words_never_blame_anyone() {
        let text = "\t at net.fabricmc.loader.impl.launch.knot.KnotClassLoader.loadClass(KnotClassLoader.java:1)";
        let tokens = candidate_tokens(Some("Fabric API"), "fabric-api-0.92.2.jar");

        assert!(
            tokens.is_empty(),
            "fabric/api are generic and must not survive: {tokens:?}"
        );
        assert!(triage(text, &[]).suspects.is_empty());
    }

    #[test]
    fn a_description_alone_still_comes_through() {
        let triage = triage("Description: Ticking entity", &[]);
        assert_eq!(triage.description.as_deref(), Some("Ticking entity"));
        assert!(triage.suspects.is_empty());
    }

    #[test]
    fn candidates_come_from_names_and_file_names_without_versions() {
        let tokens = candidate_tokens(Some("Sodium Extra"), "sodium-fabric-0.5.8.jar");
        assert!(tokens.contains(&"sodium".to_string()));
        assert!(tokens.contains(&"extra".to_string()));
        assert!(
            !tokens.iter().any(|t| t.chars().all(|c| c.is_ascii_digit())),
            "version digits are noise: {tokens:?}"
        );
    }

    #[test]
    fn each_mod_is_blamed_once_and_the_list_stays_short() {
        let text = "\
\tat com.example.one.One.f(One.java:1)
\tat com.example.two.Two.f(Two.java:1)
\tat com.example.one.One.g(One.java:2)
\tat com.example.three.Three.f(Three.java:1)
\tat com.example.four.Four.f(Four.java:1)";

        let triage = triage(
            text,
            &[
                candidate("One", &["one"]),
                candidate("Two", &["two"]),
                candidate("Three", &["three"]),
                candidate("Four", &["four"]),
            ],
        );

        assert_eq!(triage.suspects, vec!["One", "Two", "Three"]);
    }

    #[test]
    fn ordinary_prose_lines_cannot_blame_a_mod() {
        let text = "Description: install sodium alongside lithium for best results";
        let triage = triage(text, &[candidate("Sodium", &["sodium"])]);
        assert!(
            triage.suspects.is_empty(),
            "prose is not evidence: {:?}",
            triage.suspects
        );
    }

    #[test]
    fn a_jar_mention_is_evidence() {
        let text = "Caused by: java.lang.NoClassDefFoundError: com/example/mod/Thing (file:/C:/x/mods/examplemod-1.2.jar)";
        let triage = triage(text, &[candidate("ExampleMod", &["examplemod"])]);
        assert_eq!(triage.suspects, vec!["ExampleMod"]);
    }
}
