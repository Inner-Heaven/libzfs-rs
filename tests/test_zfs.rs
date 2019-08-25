#[macro_use] extern crate lazy_static;

use rand;
use slog_term;

use std::{fs::{self, DirBuilder},
          panic,
          path::{Path, PathBuf},
          sync::Mutex};

use cavity::{fill, Bytes, WriteMode};
use rand::Rng;

use libzetta::{slog::*,
               zfs::{ZfsEngine, ZfsLzc},
               zpool::{CreateVdevRequest, CreateZpoolRequest, ZpoolEngine, ZpoolOpen3}};

static ZPOOL_NAME_PREFIX: &'static str = "tests-zfs-";
lazy_static! {
    static ref INITIALIZED: Mutex<bool> = Mutex::new(false);
    static ref SHARED_ZPOOL: String = {
        let name = get_zpool_name();
        setup_zpool(&name);
        name
    };
}
fn get_zpool_name() -> String {
    let mut rng = rand::thread_rng();
    let suffix = rng.gen::<u64>();
    let name = format!("{}-{}", ZPOOL_NAME_PREFIX, suffix);
    name
}
fn get_dataset_name() -> String {
    let mut rng = rand::thread_rng();
    let name = rng.gen::<u64>();
    let name = format!("{}", name);
    name
}

fn setup_zpool(name: &str) {
    let data = INITIALIZED.lock().unwrap();

    if !*data {
        // Create vdevs if they're missing
        let vdev_dir = Path::new("/vdevs/zfs");
        setup_vdev(vdev_dir.join("vdev0"), &Bytes::MegaBytes(64 + 10));
        let zpool = ZpoolOpen3::default();
        let topo = CreateZpoolRequest::builder()
            .name(name.clone())
            .vdev(CreateVdevRequest::SingleDisk("/vdevs/zfs/vdev0".into()))
            .build()
            .unwrap();
        zpool.create(topo).unwrap();
    }
}
fn setup_vdev<P: AsRef<Path>>(path: P, bytes: &Bytes) -> PathBuf {
    let path = path.as_ref();

    let parent = path.parent().unwrap();
    DirBuilder::new().recursive(true).create(parent).unwrap();

    if path.exists() {
        let meta = fs::metadata(&path).unwrap();
        assert!(meta.is_file());
        assert!(!meta.permissions().readonly());
        if (meta.len() as usize) < bytes.as_bytes() {
            let _ = fs::remove_file(&path);
            setup_vdev(path, bytes)
        } else {
            path.into()
        }
    } else {
        let mut f = fs::File::create(path).unwrap();
        fill(bytes.clone(), None, WriteMode::FlushOnce, &mut f).unwrap();
        path.into()
    }
}
// Only used for debugging
#[allow(dead_code)]
fn get_logger() -> Logger {
    let plain = slog_term::PlainSyncDecorator::new(std::io::stdout());
    Logger::root(slog_term::FullFormat::new(plain).use_original_order().build().fuse(), o!())
}

#[test]
fn exists_on_fake() {
    let zpool = SHARED_ZPOOL.clone();
    let fake_dataset = format!("{}/very/fake/dataset", zpool);

    let zfs = ZfsLzc::new(None).expect("Failed to initialize ZfsLzc");

    let result = zfs.exists(fake_dataset).unwrap();

    assert!(!result);
}