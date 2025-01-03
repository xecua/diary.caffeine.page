use std::{
    fs::{File, OpenOptions},
    io::{BufReader, BufWriter},
    path::Path,
};

use anyhow::anyhow;
use log::info;
use serde_json::{Map, Value};

pub(super) fn load_cache(cache_file_path: &Path) -> anyhow::Result<Map<String, Value>> {
    if cache_file_path.exists() {
        let fd = File::open(cache_file_path)?;
        let reader = BufReader::new(fd);
        serde_json::from_reader(zstd::stream::decode_all(reader)?.as_slice())
            .map_err(|e| anyhow!(e))
    } else {
        info!("Cache file({cache_file_path:?}) does not exist. ignoring...",);
        Ok(Map::new())
    }
}

pub(super) fn save_cache(cache_file_path: &Path, cache: &Map<String, Value>) -> anyhow::Result<()> {
    let cache_file_fd = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(cache_file_path)?;
    let writer = BufWriter::new(cache_file_fd);
    zstd::stream::copy_encode(serde_json::to_vec(&cache)?.as_slice(), writer, 0)?;

    Ok(())
}
