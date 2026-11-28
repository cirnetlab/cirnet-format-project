use cirnet_format_project::{Limits, Manifest, decode_manifest};

fn mutations(bytes: &[u8], test: impl Fn(&[u8])) {
    let mut seed = 0x24bc_19f3u32;
    for multiplier in [1, 2, 4, 8] {
        for _ in 0..128 {
            let mut candidate = bytes.to_vec();
            for _ in 0..multiplier {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                let pos = seed as usize % candidate.len();
                candidate[pos] ^= ((seed >> 16) as u8).max(1);
            }
            test(&candidate);
        }
    }
    for end in 0..bytes.len() {
        test(&bytes[..end]);
    }
}

#[test]
fn bounded_json_mutations_preserve_canonical_round_trip_if_accepted() {
    let limits = Limits::default();
    let manifest = Manifest::new("mutations", "Mutations", vec!["electronics".into()]);
    let seed = manifest.to_bytes(&limits).unwrap();
    mutations(&seed, |input| {
        if let Ok(parsed) = decode_manifest(input, &limits) {
            assert_eq!(
                decode_manifest(&parsed.to_bytes(&limits).unwrap(), &limits).unwrap(),
                parsed
            );
        }
    });
}

#[cfg(feature = "package")]
#[test]
fn bounded_archive_mutations_keep_full_verification_at_least_as_strict_as_inspect() {
    use cirnet_format_project::{
        ProjectPath, Result,
        package::{Compression, PackageSource, export_from, inspect, verify_only},
    };
    use std::io::Cursor;
    struct EmptyProject;
    impl PackageSource for EmptyProject {
        type Reader<'a> = Cursor<&'a [u8]>;
        fn open(&self, path: &ProjectPath) -> Result<Self::Reader<'_>> {
            Err(cirnet_format_project::Error::Missing(path.to_string()))
        }
    }
    let limits = Limits::default();
    let manifest = Manifest::new("mutations", "Mutations", vec!["electronics".into()]);
    let archive = export_from(
        Cursor::new(Vec::new()),
        &manifest,
        &EmptyProject,
        Compression::Stored,
        limits,
    )
    .unwrap()
    .into_inner();
    mutations(&archive, |input| {
        if let Ok(parsed) = verify_only(Cursor::new(input), limits) {
            assert_eq!(inspect(Cursor::new(input), limits).unwrap(), parsed);
        }
    });
}
