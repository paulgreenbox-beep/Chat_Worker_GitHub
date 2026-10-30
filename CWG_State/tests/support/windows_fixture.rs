// Test-only native ACL setup, shared with the owned receiver fixtures. Do not
// include this module in a production target. It can modify only newly created
// cwg-* roots registered by this process, or their non-reparse descendants.
use std::{
    ffi::c_void,
    fs::{self, OpenOptions},
    io, mem,
    os::windows::{
        fs::{MetadataExt, OpenOptionsExt},
        io::AsRawHandle,
    },
    path::{Component, Path, PathBuf},
    ptr,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

static ROOTS: OnceLock<Mutex<Vec<PathBuf>>> = OnceLock::new();
static NONCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy)]
#[allow(dead_code)] // Each independent test target imports the shared fixture modes.
pub enum FixtureGrant {
    EveryoneRead,
    EveryoneFull,
    EveryoneCreateOnly,
    EveryoneDeleteChild,
}

pub fn private_temp_dir(label: &str) -> PathBuf {
    assert!(
        !label.is_empty()
            && label.len() <= 32
            && label
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
    );
    // Test location selection only: runtime ACL policy is always enforced.
    // This permits an explicitly approved fresh fixture below safe ancestors
    // when the host's default TEMP chain itself has unsafe application grants.
    let parent = std::env::var_os("CWG_NATIVE_FIXTURE_PARENT")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    assert!(
        parent.is_absolute()
            && !parent
                .components()
                .any(|component| matches!(component, Component::ParentDir | Component::CurDir))
    );
    for ancestor in parent.ancestors() {
        let metadata = fs::symlink_metadata(ancestor).expect("fixture parent must exist");
        assert!(
            !metadata.file_type().is_symlink() && metadata.file_attributes() & 0x400 == 0,
            "fixture parent chain cannot contain reparse aliases"
        );
    }
    let root = parent.join(format!(
        "cwg-{label}-{}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        NONCE.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&root).expect("create new owned synthetic fixture");
    ROOTS
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .unwrap()
        .push(root.clone());
    set_fixture_acl(&root, None).expect("apply actual owner-private ACL to synthetic fixture");
    root
}

pub fn set_fixture_acl(path: &Path, grant: Option<FixtureGrant>) -> io::Result<()> {
    require_owned_path(path, false)?;
    native_acl(path, grant)
}

fn require_owned_path(path: &Path, allow_absent_leaf: bool) -> io::Result<()> {
    if !path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::CurDir))
    {
        return Err(io::Error::other(
            "fixture path must be absolute without parent traversal",
        ));
    }
    let roots = ROOTS.get_or_init(|| Mutex::new(Vec::new())).lock().unwrap();
    let root = roots
        .iter()
        .find(|root| path.starts_with(root))
        .ok_or_else(|| io::Error::other("not this process's owned fixture"))?;
    for ancestor in path
        .ancestors()
        .take_while(|ancestor| ancestor.starts_with(root))
    {
        let metadata = match fs::symlink_metadata(ancestor) {
            Ok(metadata) => metadata,
            Err(error)
                if allow_absent_leaf
                    && ancestor == path
                    && error.kind() == io::ErrorKind::NotFound =>
            {
                continue;
            }
            Err(error) => return Err(error),
        };
        if metadata.file_attributes() & 0x400 != 0 || metadata.file_type().is_symlink() {
            return Err(io::Error::other(
                "fixture ACL setter refuses reparse aliases",
            ));
        }
    }
    Ok(())
}

type Handle = *mut c_void;
#[repr(C)]
#[derive(Default)]
struct FileInformation {
    attributes: u32,
    creation: [u32; 2],
    access: [u32; 2],
    write: [u32; 2],
    volume: u32,
    size_high: u32,
    size_low: u32,
    links: u32,
    index_high: u32,
    index_low: u32,
}
#[repr(C)]
struct TokenUser {
    sid: *mut c_void,
    attributes: u32,
}
#[link(name = "advapi32")]
unsafe extern "system" {
    fn OpenProcessToken(process: Handle, access: u32, token: *mut Handle) -> i32;
    fn GetTokenInformation(
        token: Handle,
        class: u32,
        buffer: *mut c_void,
        length: u32,
        returned: *mut u32,
    ) -> i32;
    fn ConvertSidToStringSidW(sid: *const c_void, text: *mut *mut u16) -> i32;
    fn ConvertStringSecurityDescriptorToSecurityDescriptorW(
        text: *const u16,
        revision: u32,
        descriptor: *mut *mut c_void,
        size: *mut u32,
    ) -> i32;
    fn GetSecurityDescriptorDacl(
        descriptor: *const c_void,
        present: *mut i32,
        dacl: *mut *mut c_void,
        defaulted: *mut i32,
    ) -> i32;
    fn SetSecurityInfo(
        handle: Handle,
        kind: u32,
        information: u32,
        owner: *mut c_void,
        group: *mut c_void,
        dacl: *mut c_void,
        sacl: *mut c_void,
    ) -> u32;
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCurrentProcess() -> Handle;
    fn CloseHandle(handle: Handle) -> i32;
    fn GetFileInformationByHandle(handle: Handle, information: *mut FileInformation) -> i32;
    fn LocalFree(memory: *mut c_void) -> *mut c_void;
    fn CreateFileMappingW(
        file: Handle,
        attributes: *const c_void,
        protection: u32,
        maximum_high: u32,
        maximum_low: u32,
        name: *const u16,
    ) -> Handle;
    fn MapViewOfFile(
        mapping: Handle,
        access: u32,
        offset_high: u32,
        offset_low: u32,
        bytes: usize,
    ) -> *mut c_void;
    fn FlushViewOfFile(view: *const c_void, bytes: usize) -> i32;
    #[allow(dead_code)] // Mapping Drop asserts release in the retained-access target.
    fn UnmapViewOfFile(view: *const c_void) -> i32;
    fn DeviceIoControl(
        device: Handle,
        code: u32,
        input: *const c_void,
        input_size: u32,
        output: *mut c_void,
        output_size: u32,
        returned: *mut u32,
        overlapped: *mut c_void,
    ) -> i32;
}
struct Local(*mut c_void);
impl Drop for Local {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.0);
        }
    }
}
struct Token(Handle);
impl Drop for Token {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
fn current_sid_string() -> io::Result<String> {
    let mut handle = ptr::null_mut();
    // SAFETY: output is writable; a successful token handle is owned by Token.
    if unsafe { OpenProcessToken(GetCurrentProcess(), 8, &mut handle) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let token = Token(handle);
    let mut required = 0;
    // SAFETY: documented null sizing call, followed by an aligned owned buffer.
    unsafe {
        GetTokenInformation(token.0, 1, ptr::null_mut(), 0, &mut required);
    }
    if !(mem::size_of::<TokenUser>() as u32..=4096).contains(&required) {
        return Err(io::Error::other("token size"));
    }
    let mut buffer = vec![0usize; (required as usize).div_ceil(mem::size_of::<usize>())];
    if unsafe {
        GetTokenInformation(
            token.0,
            1,
            buffer.as_mut_ptr().cast(),
            required,
            &mut required,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let user = unsafe { ptr::read_unaligned(buffer.as_ptr().cast::<TokenUser>()) };
    let mut text = ptr::null_mut();
    // SAFETY: TOKEN_USER's SID stays alive in buffer; API allocates terminated
    // SID text, owned by Local until it has been copied into Rust storage.
    if unsafe { ConvertSidToStringSidW(user.sid, &mut text) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let _text = Local(text.cast());
    let mut length = 0;
    while length < 256 && unsafe { *text.add(length) } != 0 {
        length += 1;
    }
    if length == 256 {
        return Err(io::Error::other("SID text length"));
    }
    Ok(String::from_utf16_lossy(unsafe {
        std::slice::from_raw_parts(text, length)
    }))
}
fn native_acl(path: &Path, grant: Option<FixtureGrant>) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    let inheritance = if metadata.is_dir() { "OICI" } else { "" };
    let user = current_sid_string()?;
    let mut sddl = format!(
        "D:P(A;{inheritance};FA;;;{user})(A;{inheritance};FA;;;SY)(A;{inheritance};FA;;;BA)"
    );
    if let Some(grant) = grant {
        let rights = match grant {
            FixtureGrant::EveryoneRead => "FR",
            FixtureGrant::EveryoneFull => "FA",
            FixtureGrant::EveryoneCreateOnly => "0x00000006",
            FixtureGrant::EveryoneDeleteChild => "0x00000040",
        };
        sddl.push_str(&format!("(A;{inheritance};{rights};;;WD)"));
    }
    let text: Vec<u16> = sddl.encode_utf16().chain(Some(0)).collect();
    let mut descriptor = ptr::null_mut();
    // SAFETY: fixed grammar plus the OS-produced SID; success returns one
    // LocalFree-owned security descriptor, alive through SetSecurityInfo.
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            text.as_ptr(),
            1,
            &mut descriptor,
            ptr::null_mut(),
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let _descriptor = Local(descriptor);
    let mut present = 0;
    let mut defaulted = 0;
    let mut dacl = ptr::null_mut();
    if unsafe { GetSecurityDescriptorDacl(descriptor, &mut present, &mut dacl, &mut defaulted) }
        == 0
        || present == 0
        || dacl.is_null()
    {
        return Err(io::Error::other("fixture DACL"));
    }
    let file = OpenOptions::new()
        .read(true)
        .access_mode(0x0002_0000 | 0x0004_0000 | 0x80)
        .share_mode(3)
        .custom_flags(0x0020_0000 | 0x0200_0000)
        .open(path)?;
    let opened = file.metadata()?;
    if opened.file_attributes() & 0x400 != 0 {
        return Err(io::Error::other("fixture reparse"));
    }
    let mut information = FileInformation::default();
    // SAFETY: exact Win32 BY_HANDLE_FILE_INFORMATION layout and a live owned
    // file handle. A multiply linked file cannot be fenced by lexical ancestry.
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut information) } == 0 {
        return Err(io::Error::last_os_error());
    }
    if opened.is_file() && information.links != 1 {
        return Err(io::Error::other("fixture hardlink alias"));
    }
    // SAFETY: this function only receives registered owned fixture paths; the
    // opened handle pins the exact object and the descriptor owns its DACL.
    let status = unsafe {
        SetSecurityInfo(
            file.as_raw_handle(),
            1,
            4 | 0x8000_0000,
            ptr::null_mut(),
            ptr::null_mut(),
            dacl,
            ptr::null_mut(),
        )
    };
    if status != 0 {
        return Err(io::Error::from_raw_os_error(status as i32));
    }
    Ok(())
}

#[allow(dead_code)] // Shared utility; only the retained-mapping target needs it.
pub struct WritableMapping {
    mapping: Handle,
    view: *mut c_void,
}
impl WritableMapping {
    #[allow(dead_code)]
    pub fn new(path: &Path) -> io::Result<Self> {
        // Enforce the same owned-root/reparse fence before touching a mapping.
        set_fixture_acl(path, Some(FixtureGrant::EveryoneFull))?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(3)
            .open(path)?;
        let length = usize::try_from(file.metadata()?.len())
            .map_err(|_| io::Error::other("fixture mapping size"))?;
        if length == 0 || length > 1024 * 1024 {
            return Err(io::Error::other("fixture mapping size"));
        }
        // SAFETY: exact owned synthetic file, nonzero bounded extent, no named
        // shared object. Both handles/views are owned and released in Drop.
        let mapping =
            unsafe { CreateFileMappingW(file.as_raw_handle(), ptr::null(), 4, 0, 0, ptr::null()) };
        if mapping.is_null() {
            return Err(io::Error::last_os_error());
        }
        let view = unsafe { MapViewOfFile(mapping, 2, 0, 0, length) };
        if view.is_null() {
            let error = io::Error::last_os_error();
            unsafe {
                CloseHandle(mapping);
            }
            return Err(error);
        }
        // file closes here: the writable section remains independently alive.
        Ok(Self { mapping, view })
    }
    #[allow(dead_code)]
    pub fn write_first_byte(&mut self, value: u8) -> io::Result<()> {
        // SAFETY: new required a nonempty writable mapped extent; self owns it.
        unsafe {
            ptr::write_volatile(self.view.cast::<u8>(), value);
        }
        if unsafe { FlushViewOfFile(self.view, 1) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}
impl Drop for WritableMapping {
    fn drop(&mut self) {
        // SAFETY: these are exactly the still-owned view and mapping handle.
        let unmapped = unsafe { UnmapViewOfFile(self.view) };
        let closed = unsafe { CloseHandle(self.mapping) };
        assert_ne!(unmapped, 0, "release owned mapping view");
        assert_ne!(closed, 0, "release owned mapping handle");
    }
}

/// Both junction and target are owned fixtures. No symlink privilege or host
/// developer-mode setting is enabled; this uses an actual mount-point ioctl.
#[allow(dead_code)] // Shared utility imported by targets without junction cases.
pub fn create_fixture_junction(link: &Path, target: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    require_owned_path(link, true)?;
    set_fixture_acl(target, None)?;
    fs::create_dir(link)?;
    set_fixture_acl(link, None)?;
    let canonical = target.canonicalize()?;
    let canonical_wide: Vec<u16> = canonical.as_os_str().encode_wide().collect();
    let verbatim: Vec<u16> = r"\\?\".encode_utf16().collect();
    let plain = canonical_wide
        .strip_prefix(verbatim.as_slice())
        .ok_or_else(|| io::Error::other("fixture local verbatim target"))?;
    let substitute: Vec<u16> = r"\??\"
        .encode_utf16()
        .chain(plain.iter().copied())
        .collect();
    let print = plain.to_vec();
    let units = substitute
        .len()
        .checked_add(print.len())
        .and_then(|value| value.checked_add(2))
        .ok_or_else(|| io::Error::other("fixture reparse size"))?;
    let total = units
        .checked_mul(2)
        .and_then(|value| value.checked_add(16))
        .filter(|value| *value <= 16 * 1024)
        .ok_or_else(|| io::Error::other("fixture reparse size"))?;
    let mut bytes = vec![0u8; total];
    bytes[0..4].copy_from_slice(&0xa000_0003u32.to_le_bytes());
    bytes[4..6].copy_from_slice(&u16::try_from(total - 8).unwrap().to_le_bytes());
    bytes[10..12].copy_from_slice(&u16::try_from(substitute.len() * 2).unwrap().to_le_bytes());
    bytes[12..14].copy_from_slice(
        &u16::try_from((substitute.len() + 1) * 2)
            .unwrap()
            .to_le_bytes(),
    );
    bytes[14..16].copy_from_slice(&u16::try_from(print.len() * 2).unwrap().to_le_bytes());
    for (index, unit) in substitute
        .iter()
        .chain(Some(&0u16))
        .chain(print.iter())
        .chain(Some(&0u16))
        .enumerate()
    {
        bytes[16 + index * 2..18 + index * 2].copy_from_slice(&unit.to_le_bytes());
    }
    let mut aligned = vec![0u32; total.div_ceil(4)];
    // SAFETY: initialized destination has at least total bytes. This is the
    // documented mount-point REPARSE_DATA_BUFFER layout with bounded UTF-16.
    unsafe {
        ptr::copy_nonoverlapping(bytes.as_ptr(), aligned.as_mut_ptr().cast::<u8>(), total);
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(3)
        .custom_flags(0x0020_0000 | 0x0200_0000)
        .open(link)?;
    let mut returned = 0;
    // SAFETY: the newly created empty directory is an owned registered fixture;
    // input is aligned and initialized; a synchronous ioctl has no output data.
    if unsafe {
        DeviceIoControl(
            file.as_raw_handle(),
            0x0009_00a4,
            aligned.as_ptr().cast(),
            total as u32,
            ptr::null_mut(),
            0,
            &mut returned,
            ptr::null_mut(),
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
