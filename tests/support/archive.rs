use cirnet_format_project::package::{INVENTORY_ENTRY, MANIFEST_ENTRY};
use std::io::{Cursor, Write};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

pub(crate) fn with_inventory(manifest: &[u8], inventory: &[u8]) -> Cursor<Vec<u8>> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    for (name, bytes) in [(MANIFEST_ENTRY, manifest), (INVENTORY_ENTRY, inventory)] {
        zip.start_file(name, options).unwrap();
        zip.write_all(bytes).unwrap();
    }
    let mut archive = zip.finish().unwrap();
    archive.set_position(0);
    archive
}
