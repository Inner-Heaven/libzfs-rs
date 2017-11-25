extern crate libzfs;
extern crate tempdir;
extern crate slog_term;
extern crate cavity;

use libzfs::slog::*;
use libzfs::zpool::{Disk, TopologyBuilder, Vdev, ZpoolEngine, ZpoolOpen3};
use libzfs::zpool::{ZpoolError, ZpoolErrorKind};

use cavity::{fill, WriteMode, Bytes};

use std::panic;
use std::path::{Path, PathBuf};
use std::fs;

static ZPOOL_NAME: &'static str = "tests";

fn setup_vdev<P: AsRef<Path>>(path: P, bytes: &Bytes) -> PathBuf{
    let path = path.as_ref();
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
fn setup() {
    let vdev_dir = Path::new("/vdevs");
    let vdev0 = setup_vdev(vdev_dir.join("vdev0"), &Bytes::MegaBytes(64 + 10));
    let vdev1 = setup_vdev(vdev_dir.join("vdev1"), &Bytes::MegaBytes(64 + 10));
    let vdev2 = setup_vdev(vdev_dir.join("vdev2"), &Bytes::MegaBytes(1));
}
fn teardown() {
    let z = ZpoolOpen3::default();
    if z.exists(&ZPOOL_NAME).unwrap() {
        z.destroy(&ZPOOL_NAME, true);
    }
}

fn run_test<T>(test: T)
    where T: FnOnce() -> () + panic::UnwindSafe {
        setup();

        let result = panic::catch_unwind(||{
            test()
        });

        teardown();

        result.unwrap();
}
fn get_logger() -> Logger {
    let plain = slog_term::PlainSyncDecorator::new(std::io::stdout());
    Logger::root(slog_term::FullFormat::new(plain).build().fuse(), o!())
}

#[test]
fn create_check_delete() {
    run_test(|| {
        let zpool = ZpoolOpen3::with_logger(get_logger());
        let name = ZPOOL_NAME;


        let topo = TopologyBuilder::default()
            .vdev(Vdev::Naked(Disk::File("/vdevs/vdev0".into())))
            .build()
            .unwrap();

        zpool.create(&name, topo).unwrap();

        let result = zpool.exists(&name).unwrap();
        assert!(result);

        zpool.destroy(&name, true).unwrap();

        let result = zpool.exists(&name).unwrap();
        assert!(!result);
    })
}


#[test]
fn cmd_not_found() {
    run_test(|| {
        let zpool = ZpoolOpen3::with_cmd("zpool-not-found");
        let name = ZPOOL_NAME;

        let topo = TopologyBuilder::default()
            .vdev(Vdev::Naked(Disk::File("/vdevs/vdev0".into())))
            .build()
            .unwrap();

        let result = zpool.create(&name, topo);
        assert_eq!(ZpoolErrorKind::CmdNotFound, result.unwrap_err().kind());
    })
}

#[test]
fn reuse_vdev() {
    run_test(|| {
        let zpool = ZpoolOpen3::default();
        let name_1 = ZPOOL_NAME;
        let name_2 = "zpool-tests-fail";
        let vdev_file = "/vdevs/vdev0";

        let topo = TopologyBuilder::default()
            .vdev(Vdev::Naked(Disk::File(vdev_file.into())))
            .build()
            .unwrap();

        let result = zpool.create(&name_1, topo.clone());
        result.unwrap();
        let result = zpool.create(&name_2, topo.clone());
        let err = result.unwrap_err();
        assert_eq!(ZpoolErrorKind::VdevReuse, err.kind());
        println!("{:?}", &err);
        if let ZpoolError::VdevReuse(vdev, pool) = err {
            assert_eq!(vdev_file, vdev);
            assert_eq!(name_1, pool);
        }
        zpool.destroy(&name_1, true).unwrap();
    });
}
