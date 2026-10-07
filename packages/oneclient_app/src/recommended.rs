use oneclient_cluster::Cluster;
use oneclient_content::packages::{ContentType, GameLoader, ProviderId};

/// A hand-picked catalog of widely used client-side packages, offered as the Recommended tab.
/// Ids are Modrinth project ids verified against the live API — never guess them when editing.
pub struct RecommendedPackage {
    pub provider: ProviderId,
    pub project_id: &'static str,
    pub content_type: ContentType,
    pub name: &'static str,
    pub summary: &'static str,
    pub loaders: &'static [GameLoader],
    /// Earliest Minecraft version the package still supports a cluster below this never sees the row
    pub since: &'static str,
}

pub const RECOMMENDED_PACKAGES: &[RecommendedPackage] = &[
    RecommendedPackage {
        provider: ProviderId::Modrinth,
        project_id: "AANobbMI",
        content_type: ContentType::Mod,
        name: "Sodium",
        summary: "A high-performance rendering engine replacement that greatly improves frame rates.",
        loaders: &[GameLoader::Fabric, GameLoader::NeoForge, GameLoader::Quilt],
        since: "1.16.3",
    },
    RecommendedPackage {
        provider: ProviderId::Modrinth,
        project_id: "gvQqBUqZ",
        content_type: ContentType::Mod,
        name: "Lithium",
        summary: "No-compromises game logic optimization, useful for single-player and servers.",
        loaders: &[GameLoader::Fabric, GameLoader::NeoForge, GameLoader::Quilt],
        since: "1.16.2",
    },
    RecommendedPackage {
        provider: ProviderId::Modrinth,
        project_id: "YL57xq9U",
        content_type: ContentType::Mod,
        name: "Iris Shaders",
        summary: "A modern shader pack loader, compatible with most OptiFine shader packs.",
        loaders: &[GameLoader::Fabric, GameLoader::NeoForge, GameLoader::Quilt],
        since: "1.16.5",
    },
    RecommendedPackage {
        provider: ProviderId::Modrinth,
        project_id: "mOgUt4GM",
        content_type: ContentType::Mod,
        name: "Mod Menu",
        summary: "Adds a mod menu to view the list of mods you have installed.",
        loaders: &[GameLoader::Fabric, GameLoader::Quilt],
        since: "1.14.4",
    },
    RecommendedPackage {
        provider: ProviderId::Modrinth,
        project_id: "EsAfCjCV",
        content_type: ContentType::Mod,
        name: "AppleSkin",
        summary: "Food/hunger-related HUD improvements.",
        loaders: &[
            GameLoader::Fabric,
            GameLoader::Forge,
            GameLoader::NeoForge,
            GameLoader::Quilt,
        ],
        since: "1.10.2",
    },
    RecommendedPackage {
        provider: ProviderId::Modrinth,
        project_id: "w7ThoJFB",
        content_type: ContentType::Mod,
        name: "Zoomify",
        summary: "A zoom mod with infinite customizability.",
        loaders: &[GameLoader::Fabric, GameLoader::Quilt],
        since: "1.18",
    },
    RecommendedPackage {
        provider: ProviderId::Modrinth,
        project_id: "NNAgCjsB",
        content_type: ContentType::Mod,
        name: "Entity Culling",
        summary: "Hides block entities and entities that are not visible to boost performance.",
        loaders: &[
            GameLoader::Fabric,
            GameLoader::Forge,
            GameLoader::NeoForge,
            GameLoader::Quilt,
        ],
        since: "1.7.10",
    },
    RecommendedPackage {
        provider: ProviderId::Modrinth,
        project_id: "uXXizFIs",
        content_type: ContentType::Mod,
        name: "FerriteCore",
        summary: "Memory usage optimizations.",
        loaders: &[
            GameLoader::Fabric,
            GameLoader::Forge,
            GameLoader::NeoForge,
            GameLoader::Quilt,
        ],
        since: "1.16.5",
    },
    RecommendedPackage {
        provider: ProviderId::Modrinth,
        project_id: "5ZwdcRci",
        content_type: ContentType::Mod,
        name: "ImmediatelyFast",
        summary: "Speeds up immediate mode rendering across the UI and world.",
        loaders: &[
            GameLoader::Fabric,
            GameLoader::Forge,
            GameLoader::NeoForge,
            GameLoader::Quilt,
        ],
        since: "1.18.2",
    },
    RecommendedPackage {
        provider: ProviderId::Modrinth,
        project_id: "8shC1gFX",
        content_type: ContentType::Mod,
        name: "BetterF3",
        summary: "Replaces the debug HUD with a customizable, human-readable one.",
        loaders: &[
            GameLoader::Fabric,
            GameLoader::Forge,
            GameLoader::NeoForge,
            GameLoader::Quilt,
        ],
        since: "1.16",
    },
    RecommendedPackage {
        provider: ProviderId::Modrinth,
        project_id: "LQ3K71Q1",
        content_type: ContentType::Mod,
        name: "Dynamic FPS",
        summary: "Reduces resource usage while Minecraft is in the background or idle.",
        loaders: &[
            GameLoader::Fabric,
            GameLoader::Forge,
            GameLoader::NeoForge,
            GameLoader::Quilt,
        ],
        since: "1.14.3",
    },
    RecommendedPackage {
        provider: ProviderId::Modrinth,
        project_id: "yBW8D80W",
        content_type: ContentType::Mod,
        name: "LambDynamicLights",
        summary: "Adds feature-complete, optimized dynamic lighting.",
        loaders: &[GameLoader::Fabric, GameLoader::NeoForge, GameLoader::Quilt],
        since: "1.16.2",
    },
    RecommendedPackage {
        provider: ProviderId::Modrinth,
        project_id: "qQyHxfxd",
        content_type: ContentType::Mod,
        name: "No Chat Reports",
        summary: "Makes chat unreportable where possible.",
        loaders: &[
            GameLoader::Fabric,
            GameLoader::Forge,
            GameLoader::NeoForge,
            GameLoader::Quilt,
        ],
        since: "1.19",
    },
    RecommendedPackage {
        provider: ProviderId::Modrinth,
        project_id: "u6dRKJwZ",
        content_type: ContentType::Mod,
        name: "Just Enough Items (JEI)",
        summary: "View items and recipes.",
        loaders: &[GameLoader::Fabric, GameLoader::Forge, GameLoader::NeoForge],
        since: "1.8",
    },
    RecommendedPackage {
        provider: ProviderId::Modrinth,
        project_id: "fRiHVvU7",
        content_type: ContentType::Mod,
        name: "EMI",
        summary: "A featureful and accessible item and recipe viewer.",
        loaders: &[
            GameLoader::Fabric,
            GameLoader::Forge,
            GameLoader::NeoForge,
            GameLoader::Quilt,
        ],
        since: "1.18.2",
    },
    RecommendedPackage {
        provider: ProviderId::Modrinth,
        project_id: "1bokaNcj",
        content_type: ContentType::Mod,
        name: "Xaero's Minimap",
        summary: "A minimap with waypoints in the corner of your screen.",
        loaders: &[
            GameLoader::Fabric,
            GameLoader::Forge,
            GameLoader::NeoForge,
            GameLoader::Quilt,
        ],
        since: "1.7.10",
    },
];

/// Catalog picks that suit this cluster — the route's content type first, then loader, then the MC floor.
/// An unknown cluster offers nothing: the chip only appears once the launcher knows what it is running.
pub fn for_cluster(
    content_type: ContentType,
    cluster: Option<&Cluster>,
) -> Vec<&'static RecommendedPackage> {
    let Some(cluster) = cluster else {
        return Vec::new();
    };
    RECOMMENDED_PACKAGES
        .iter()
        .filter(|p| p.content_type == content_type)
        .filter(|p| loader_ok(p, cluster.mc_loader) && mc_at_least(&cluster.mc_version, p.since))
        .collect()
}

/// Every catalog project id for a content type feeds the meta batch so synthesized rows get icons
pub fn project_ids(content_type: ContentType, provider: ProviderId) -> Vec<String> {
    RECOMMENDED_PACKAGES
        .iter()
        .filter(|p| p.content_type == content_type && p.provider == provider)
        .map(|p| p.project_id.to_string())
        .collect()
}

fn loader_ok(p: &RecommendedPackage, loader: GameLoader) -> bool {
    p.loaders.is_empty() || p.loaders.contains(&loader)
}

/// A numeric dotted `have >= since` snapshots and other odd version strings stay visible the
/// install button narrows by exact compatibility anyway so the row decides rather than vanishing
fn mc_at_least(have: &str, since: &str) -> bool {
    let (Ok(have), Ok(since)) = (parse_version(have), parse_version(since)) else {
        return true;
    };
    let len = have.len().max(since.len());
    for i in 0..len {
        let h = have.get(i).copied().unwrap_or(0);
        let s = since.get(i).copied().unwrap_or(0);
        if h != s {
            return h > s;
        }
    }
    true
}

fn parse_version(version: &str) -> Result<Vec<u32>, std::num::ParseIntError> {
    version.split('.').map(str::parse).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_catalog_never_ships_with_duplicated_or_blank_entries() {
        let mut ids: Vec<&str> = RECOMMENDED_PACKAGES.iter().map(|p| p.project_id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), RECOMMENDED_PACKAGES.len());

        for p in RECOMMENDED_PACKAGES {
            assert!(!p.name.is_empty());
            assert!(!p.summary.is_empty());
            assert!(!p.loaders.is_empty());
            assert_eq!(p.provider, ProviderId::Modrinth);
            assert_eq!(p.content_type, ContentType::Mod);
        }
    }

    #[test]
    fn the_mc_floor_hides_old_clusters_but_lets_snapshots_through() {
        assert!(mc_at_least("1.21.4", "1.16.3"));
        assert!(mc_at_least("1.16", "1.16.0"));
        assert!(mc_at_least("1.16.1", "1.16"));
        assert!(mc_at_least("26.1.2", "1.21"));
        assert!(!mc_at_least("1.15.2", "1.16"));
        assert!(!mc_at_least("1.12.2", "1.16"));
        assert!(mc_at_least("25w31a", "1.21"), "snapshots stay visible");
    }

    #[test]
    fn a_loader_mismatch_takes_the_row_off_the_tab() {
        let sodium = &RECOMMENDED_PACKAGES[0];
        assert!(loader_ok(sodium, GameLoader::Fabric));
        assert!(loader_ok(sodium, GameLoader::NeoForge));
        assert!(!loader_ok(sodium, GameLoader::Forge));
        assert!(!loader_ok(sodium, GameLoader::Vanilla));
    }
}
