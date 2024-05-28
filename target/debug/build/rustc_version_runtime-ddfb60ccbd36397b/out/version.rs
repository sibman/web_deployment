
            /// Returns the `rustc` SemVer version and additional metadata
            /// like the git short hash and build date.
            pub fn version_meta() -> VersionMeta {
                VersionMeta {
                    semver: Version {
                        major: 1,
                        minor: 80,
                        patch: 0,
                        pre: vec![semver::Identifier::AlphaNumeric("nightly".to_owned()), ],
                        build: vec![],
                    },
                    host: "x86_64-unknown-linux-gnu".to_owned(),
                    short_version_string: "rustc 1.80.0-nightly (1ba35e9bb 2024-05-25)".to_owned(),
                    commit_hash: Some("1ba35e9bb44d416fc2ebf897855454258b650b01".to_owned()),
                    commit_date: Some("2024-05-25".to_owned()),
                    build_date: None,
                    channel: Channel::Nightly,
                }
            }
            