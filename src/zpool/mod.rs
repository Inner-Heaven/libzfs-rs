/// Everything you need to work with zpools. Since there is no public library
/// to work with zpool —
/// the default impl will call to `zpool(8)`.
use std::io;
use std::num::{ParseFloatError, ParseIntError};
use std::path::PathBuf;

use regex::Regex;

pub use self::description::Zpool;
pub use self::open3::ZpoolOpen3;
pub use self::properties::{
    CacheType, FailMode, Health, PropPair, ZpoolProperties, ZpoolPropertiesWrite,
    ZpoolPropertiesWriteBuilder,
};
pub use self::topology::{Topology, TopologyBuilder};
pub use self::vdev::{Disk, Vdev};

pub mod vdev;
pub mod topology;
pub mod open3;
pub mod properties;

pub mod description;
lazy_static! {
    static ref RE_REUSE_VDEV_ZOL: Regex = Regex::new(r"cannot create \S+: one or more vdevs refer to the same device, or one of\nthe devices is part of an active md or lvm device\n").expect("failed to compile RE_VDEV_REUSE_ZOL)");
    static ref RE_REUSE_VDEV: Regex = Regex::new(r"following errors:\n(\S+) is part of active pool '(\S+)'").expect("failed to compile RE_VDEV_REUSE)");
    static ref RE_TOO_SMALL: Regex = Regex::new(r"cannot create \S+: one or more devices is less than the minimum size \S+").expect("failed to compile RE_TOO_SMALL");
    static ref RE_PERMISSION_DENIED: Regex = Regex::new(r"cannot create \S+: permission denied\n").expect("failed to compile RE_PERMISSION_DENIED");
}

quick_error! {
    /// Error kinds. This type will be used across zpool module.
    #[derive(Debug)]
    pub enum ZpoolError {
        /// `zpool` not found in path. Open3 specific error.
        CmdNotFound {}
        /// zpool executable not found in path.
        Io(err: io::Error) {
            cause(err)
        }
        /// Trying to manipulate non-existent pool.
        PoolNotFound {}
        /// Given topology failed validation.
        InvalidTopology {}
        /// Trying to create new Zpool, but one or more vdevs already used in another pool.
        VdevReuse(vdev: String, pool: String) {
            display("{} is part of {}", vdev, pool)
        }
        /// Failed to parse value. Ideally you never see it, if you see it - it's a bug.
        ParseError {
            from(ParseIntError)
            from(ParseFloatError)
        }
        /// Device used in Topology is smaller than 64M
        DeviceTooSmall {}
        /// Permission denied to create zpool. This might happened because:
        /// a) you running it as not root
        /// b) you running it inside jail that isn't allowed to operate zfs
        PermissionDenied {}
        /// Don't know (yet) how to categorize this error. If you see this error - open an issues.
        Other(err: String) {}
    }
}

impl ZpoolError {
    pub fn kind(&self) -> ZpoolErrorKind {
        match *self {
            ZpoolError::CmdNotFound => ZpoolErrorKind::CmdNotFound,
            ZpoolError::Io(_) => ZpoolErrorKind::Io,
            ZpoolError::PoolNotFound => ZpoolErrorKind::PoolNotFound,
            ZpoolError::InvalidTopology => ZpoolErrorKind::InvalidTopology,
            ZpoolError::VdevReuse(_, _) => ZpoolErrorKind::VdevReuse,
            ZpoolError::ParseError => ZpoolErrorKind::ParseError,
            ZpoolError::DeviceTooSmall => ZpoolErrorKind::DeviceTooSmall,
            ZpoolError::PermissionDenied => ZpoolErrorKind::PermissionDenied,
            ZpoolError::Other(_) => ZpoolErrorKind::Other,
        }
    }
}

/// This is a hack to allow error identification without 100500 lines of code
/// because
/// `std::io::Error` doesn't implement `PartialEq`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ZpoolErrorKind {
    /// `zpool` not found in path. Open3 specific error.
    CmdNotFound,
    Io,
    /// Trying to manipulate non-existent pool.
    PoolNotFound,
    /// At least one vdev points to incorrect location.
    /// If vdev type is File then it means file not found.
    DeviceNotFound,
    /// Trying to create new Zpool, but one or more vdevs already used in
    /// another pool.
    VdevReuse,
    /// Given topology failed validation.
    InvalidTopology,
    /// Failed to parse value. Ideally you never see it, if you see it - it's a
    /// bug.
    ParseError,
    /// Device used in Topology is smaller than 64M
    DeviceTooSmall,
    /// Permission denied to create zpool. This might happened because:
    /// a) you running it as not root
    /// b) you running it inside jail that isn't allowed to operate zfs
    PermissionDenied,
    /// Don't know (yet) how to categorize this error. If you see this error -
    /// open an issues.
    Other,
}

impl From<io::Error> for ZpoolError {
    fn from(err: io::Error) -> ZpoolError {
        match err.kind() {
            io::ErrorKind::NotFound => ZpoolError::CmdNotFound,
            _ => ZpoolError::Io(err),
        }
    }
}

impl ZpoolError {
    /// Try to convert stderr into internal error type.
    pub fn from_stderr(stderr_raw: &[u8]) -> ZpoolError {
        let stderr = String::from_utf8_lossy(stderr_raw);
        if RE_REUSE_VDEV.is_match(&stderr) {
            let caps = RE_REUSE_VDEV.captures(&stderr).unwrap();
            ZpoolError::VdevReuse(
                caps.get(1).unwrap().as_str().into(),
                caps.get(2).unwrap().as_str().into(),
            )
        } else if RE_REUSE_VDEV_ZOL.is_match(&stderr) {
            ZpoolError::VdevReuse(String::new(), String::new())
        } else if RE_TOO_SMALL.is_match(&stderr) {
            ZpoolError::DeviceTooSmall
        } else if RE_PERMISSION_DENIED.is_match(&stderr) {
            ZpoolError::PermissionDenied
        } else {
            ZpoolError::Other(stderr.into())
        }
    }
}

/// Type alias to `Result<T, ZpoolError>`.
pub type ZpoolResult<T> = Result<T, ZpoolError>;

/// Generic interface to manage zpools. End goal is to cover most of `zpool(8)`.
/// Using trait here, so I can mock it in unit tests.
pub trait ZpoolEngine {
    /// Check if pool with given name exists. This should not return
    /// [`ZpoolError::PoolNotFound`](enum.ZpoolError.html) error, instead
    /// it should return `Ok(false)`.
    fn exists<N: AsRef<str>>(&self, name: N) -> ZpoolResult<bool>;
    /// Version of create that doesn't check validness of topology or options.
    fn create_unchecked<
        N: AsRef<str>,
        P: Into<Option<ZpoolPropertiesWrite>>,
        M: Into<Option<PathBuf>>,
        A: Into<Option<PathBuf>>,
    >(
        &self,
        name: N,
        topology: Topology,
        props: P,
        mount: M,
        alt_root: A,
    ) -> ZpoolResult<()>;
    /// Create new zpool.
    fn create<
        N: AsRef<str>,
        P: Into<Option<ZpoolPropertiesWrite>>,
        M: Into<Option<PathBuf>>,
        A: Into<Option<PathBuf>>,
    >(
        &self,
        name: N,
        topology: Topology,
        props: P,
        mount: M,
        alt_root: A,
    ) -> ZpoolResult<()> {
        if !topology.is_suitable_for_create() {
            return Err(ZpoolError::InvalidTopology);
        }
        self.create_unchecked(name, topology, props, mount, alt_root)
    }
    /// Version of destroy that doesn't verify if pool exists before removing
    /// it.
    fn destroy_unchecked<N: AsRef<str>>(&self, name: N, force: bool) -> ZpoolResult<()>;
    /// Destroy zpool.
    fn destroy<N: AsRef<str>>(&self, name: N, force: bool) -> ZpoolResult<()> {
        if !self.exists(&name)? {
            return Err(ZpoolError::PoolNotFound);
        }

        self.destroy_unchecked(name, force)
    }
    /// Read properties of the pool.
    fn read_properties_unchecked<N: AsRef<str>>(&self, name: N) -> ZpoolResult<ZpoolProperties>;
    /// Ditto
    fn read_properties<N: AsRef<str>>(&self, name: N) -> ZpoolResult<ZpoolProperties> {
        if !self.exists(&name)? {
            return Err(ZpoolError::PoolNotFound);
        }
        self.read_properties_unchecked(name)
    }

    /// Update zpool properties.
    fn update_properties<N: AsRef<str>>(
        &self,
        name: N,
        props: ZpoolPropertiesWrite,
    ) -> ZpoolResult<ZpoolProperties> {
        if !self.exists(&name)? {
            return Err(ZpoolError::PoolNotFound);
        }

        let current = self.read_properties_unchecked(&name)?;

        if current.auto_expand != *props.auto_expand() {
            self.set_unchecked(&name, "autoexpand", props.auto_expand())?;
        }

        if current.auto_replace != *props.auto_replace() {
            self.set_unchecked(&name, "autoreplace", props.auto_replace())?;
        }

        if current.cache_file != *props.cache_file() {
            self.set_unchecked(&name, "cachefile", props.cache_file())?;
        }

        // remove comment
        let desired = if props.comment().is_empty() {
            None
        } else {
            Some(props.comment().clone())
        };
        if current.comment != desired {
            self.set_unchecked(&name, "comment", props.comment())?;
        }

        if current.delegation != *props.delegation() {
            self.set_unchecked(&name, "delegation", props.delegation())?;
        }

        if current.fail_mode != *props.fail_mode() {
            self.set_unchecked(&name, "failmode", props.fail_mode())?;
        }

        self.read_properties_unchecked(name)
    }

    /// Internal function used to set values. Should be avoided.
    fn set_unchecked<N: AsRef<str>, P: PropPair>(
        &self,
        name: N,
        key: &str,
        value: &P,
    ) -> ZpoolResult<()>;
    /// Export Pool.
    fn export<N: AsRef<str>>(&self, name: N, force: bool) -> ZpoolResult<()> {
        if !self.exists(&name)? {
            return Err(ZpoolError::PoolNotFound);
        }
        self.export_unchecked(name, force)
    }

    fn export_unchecked<N: AsRef<str>>(&self, name: N, force: bool) -> ZpoolResult<()>;
    /// List of pools available for import in `/dev/` directory.
    fn available(&self) -> ZpoolResult<Vec<Zpool>>;
    /// List of pools available
    fn available_in_dir(&self, dir: PathBuf) -> ZpoolResult<Vec<Zpool>>;

    /// Import pool
    fn import_from_dir<N: AsRef<str>>(&self, name: N, dir: PathBuf) -> ZpoolResult<()>;

    /// Status of a single pool
    fn status_unchecked<N: AsRef<str>>(&self, name: N) -> ZpoolResult<Zpool>;

    /// Status of a single pool
    fn status<N: AsRef<str>>(&self, name: N) -> ZpoolResult<Zpool> {
        if !self.exists(&name)? {
            return Err(ZpoolError::PoolNotFound);
        }
        self.status_unchecked(name)
    }
    /// Get a status of each pool active in the system
    fn all(&self) -> ZpoolResult<Vec<Zpool>>;
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn error_parsing() {
        let vdev_reuse_text = b"invalid vdev specification\nuse '-f' to override the following errors:\n/vdevs/vdev0 is part of active pool 'tank'";
        let unknown_text = b"wat";

        let err = ZpoolError::from_stderr(vdev_reuse_text);

        assert_eq!(ZpoolErrorKind::VdevReuse, err.kind());
        if let ZpoolError::VdevReuse(vdev, pool) = err {
            assert_eq!("/vdevs/vdev0", vdev);
            assert_eq!("tank", pool);
        }

        let err = ZpoolError::from_stderr(unknown_text);
        assert_eq!(ZpoolErrorKind::Other, err.kind());
        if let ZpoolError::Other(text) = err {
            assert_eq!("wat", text);
        }

        let vdev_reuse_text = b"cannot create \'tests-8804202574521870666\': one or more vdevs refer to the same device, or one of\nthe devices is part of an active md or lvm device\n";
        let err = ZpoolError::from_stderr(vdev_reuse_text);
        assert_eq!(ZpoolErrorKind::VdevReuse, err.kind());
    }

    #[test]
    fn io_error_from() {
        let cmd_not_found = io::Error::new(io::ErrorKind::NotFound, "oh no");
        let err = ZpoolError::from(cmd_not_found);
        assert_eq!(ZpoolErrorKind::CmdNotFound, err.kind());

        let other = io::Error::new(io::ErrorKind::Other, "oh now");
        let err = ZpoolError::from(other);
        assert_eq!(ZpoolErrorKind::Io, err.kind());
    }

    //noinspection RsTypeCheck
    #[test]
    fn num_error_from() {
        let int_err = "as".parse::<i8>().unwrap_err();
        let float_err = "as".parse::<f32>().unwrap_err();

        let err = ZpoolError::from(int_err);
        assert_eq!(ZpoolErrorKind::ParseError, err.kind());

        let err = ZpoolError::from(float_err);
        assert_eq!(ZpoolErrorKind::ParseError, err.kind());
    }

    #[test]
    fn too_small() {
        let text = b"cannot create \'tests-5825559772339520034\': one or more devices is less than the minimum size (64M)\n";
        let err = ZpoolError::from_stderr(text);

        assert_eq!(ZpoolErrorKind::DeviceTooSmall, err.kind());
    }

    #[test]
    fn permission_denied() {
        let text = b"cannot create \'tests-10742509212158788460\': permission denied\n";
        let err = ZpoolError::from_stderr(text);

        assert_eq!(ZpoolErrorKind::PermissionDenied, err.kind());
    }
}
