//! Windows handle-based private-file checks. These functions inspect real
//! owners/DACLs; they never repair ACLs, enable privileges or trust a config flag.
//! Current user, LocalSystem and Builtin Administrators are trusted for private
//! data. Ancestors additionally permit the resolved TrustedInstaller service.
//! Same-user/administrator compromise and hostile filesystem drivers remain
//! outside this single-host boundary.
//!
//! References: GetSecurityInfo, File Security and Access Rights, CreateFileW and
//! GetFileInformationByHandle in the Microsoft Win32 documentation.
use std::{fs::File, path::Path};

pub type Result<T> = std::result::Result<T, &'static str>;

/// Keep this guard alive through every read/lock/append/sync operation. Its
/// ancestor and leaf handles omit FILE_SHARE_DELETE, pinning the checked names.
pub struct GuardedFile {
    file: File,
    _directories: Vec<File>,
}
impl GuardedFile {
    pub fn file_mut(&mut self) -> &mut File {
        &mut self.file
    }
}

#[cfg(not(windows))]
pub fn read_private_config(_: &Path, _: usize) -> Result<Vec<u8>> {
    Err("WINDOWS_ACL_REQUIRED")
}
#[cfg(not(windows))]
pub fn open_private_journal(_: &Path) -> Result<GuardedFile> {
    Err("WINDOWS_ACL_REQUIRED")
}
#[cfg(not(windows))]
pub fn validate_private_file_handle(_: &File) -> Result<()> {
    Err("WINDOWS_ACL_REQUIRED")
}

#[cfg(windows)]
pub use native::{open_private_journal, read_private_config, validate_private_file_handle};

#[cfg(windows)]
mod native {
    use super::{GuardedFile, Result};
    use std::{
        ffi::c_void,
        fs::{File, OpenOptions},
        io::Read,
        mem,
        os::windows::{ffi::OsStrExt, fs::OpenOptionsExt, io::AsRawHandle},
        path::{Component, Path, PathBuf, Prefix},
        ptr,
    };

    type Handle = *mut c_void;
    const READ_CONTROL: u32 = 0x0002_0000;
    const FILE_READ_ATTRIBUTES: u32 = 0x80;
    const OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const REPARSE_ATTRIBUTE: u32 = 0x400;
    const DIRECTORY_ATTRIBUTE: u32 = 0x10;
    const OWNER_INFORMATION: u32 = 1;
    const DACL_INFORMATION: u32 = 4;
    const FILE_OBJECT: u32 = 1;
    const TOKEN_QUERY: u32 = 8;
    const ERROR_NO_TOKEN: u32 = 1008;
    const INHERIT_ONLY_ACE: u8 = 8;
    // Existing children are pinned by non-delete-sharing handles. Adding a new
    // unrelated file/directory does not replace them. Write attributes/EA,
    // DELETE/DELETE_CHILD, owner/DACL and unknown rights are intentionally absent.
    const SAFE_ANCESTOR_RIGHTS: u32 = 1 | 2 | 4 | 8 | 0x20 | 0x80
        | READ_CONTROL | 0x0010_0000 | 0x8000_0000 | 0x2000_0000;

    #[repr(C)]
    struct Acl {
        revision: u8,
        reserved: u8,
        size: u16,
        ace_count: u16,
        reserved2: u16,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct AceHeader {
        kind: u8,
        flags: u8,
        size: u16,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct SidAndAttributes {
        sid: *mut c_void,
        attributes: u32,
    }
    #[repr(C)]
    #[derive(Default)]
    struct FileTime {
        low: u32,
        high: u32,
    }
    #[repr(C)]
    #[derive(Default)]
    struct FileInformation {
        attributes: u32,
        creation: FileTime,
        access: FileTime,
        write: FileTime,
        volume: u32,
        size_high: u32,
        size_low: u32,
        links: u32,
        index_high: u32,
        index_low: u32,
    }

    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn GetSecurityInfo(handle: Handle, kind: u32, information: u32,
            owner: *mut *mut c_void, group: *mut *mut c_void, dacl: *mut *mut Acl,
            sacl: *mut *mut Acl, descriptor: *mut *mut c_void) -> u32;
        fn IsValidSecurityDescriptor(descriptor: *const c_void) -> i32;
        fn GetSecurityDescriptorDacl(descriptor: *const c_void, present: *mut i32,
            dacl: *mut *mut Acl, defaulted: *mut i32) -> i32;
        fn IsValidAcl(acl: *const Acl) -> i32;
        fn GetAce(acl: *const Acl, index: u32, ace: *mut *mut c_void) -> i32;
        fn IsValidSid(sid: *const c_void) -> i32;
        fn GetLengthSid(sid: *const c_void) -> u32;
        fn EqualSid(left: *const c_void, right: *const c_void) -> i32;
        fn ConvertStringSidToSidW(text: *const u16, sid: *mut *mut c_void) -> i32;
        fn OpenProcessToken(process: Handle, access: u32, token: *mut Handle) -> i32;
        fn OpenThreadToken(thread: Handle, access: u32, as_self: i32, token: *mut Handle) -> i32;
        fn GetTokenInformation(token: Handle, class: u32, buffer: *mut c_void,
            length: u32, returned: *mut u32) -> i32;
        fn LookupAccountNameW(system: *const u16, account: *const u16, sid: *mut c_void,
            sid_size: *mut u32, domain: *mut u16, domain_size: *mut u32, use_kind: *mut u32) -> i32;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn LocalFree(memory: *mut c_void) -> *mut c_void;
        fn CloseHandle(handle: Handle) -> i32;
        fn GetCurrentProcess() -> Handle;
        fn GetCurrentThread() -> Handle;
        fn GetLastError() -> u32;
        fn GetFileType(handle: Handle) -> u32;
        fn GetDriveTypeW(root: *const u16) -> u32;
        fn GetFileInformationByHandle(handle: Handle, information: *mut FileInformation) -> i32;
        fn GetVolumeInformationByHandleW(handle: Handle, name: *mut u16, name_size: u32,
            serial: *mut u32, component_length: *mut u32, flags: *mut u32,
            filesystem: *mut u16, filesystem_size: u32) -> i32;
    }

    struct LocalAllocation(*mut c_void);
    impl Drop for LocalAllocation {
        fn drop(&mut self) {
            // SAFETY: this allocation was returned by a LocalAlloc-family API
            // and is owned by this guard. Component pointers are not freed.
            unsafe { LocalFree(self.0); }
        }
    }
    struct Token(Handle);
    impl Drop for Token {
        fn drop(&mut self) {
            // SAFETY: this is an owned real token handle, not a pseudo handle.
            unsafe { CloseHandle(self.0); }
        }
    }
    struct Sid(Vec<u32>);
    impl Sid {
        fn pointer(&self) -> *const c_void { self.0.as_ptr().cast() }
    }
    fn wide(text: &str) -> Vec<u16> { text.encode_utf16().chain(Some(0)).collect() }
    fn fixed_sid(text: &str) -> Result<LocalAllocation> {
        let mut sid = ptr::null_mut();
        let text = wide(text);
        // SAFETY: text is terminated and sid is writable; success allocates a
        // complete SID which remains owned until LocalAllocation is dropped.
        if unsafe { ConvertStringSidToSidW(text.as_ptr(), &mut sid) } == 0 || sid.is_null() {
            return Err("ACL_SID_QUERY");
        }
        Ok(LocalAllocation(sid))
    }
    fn current_user() -> Result<Sid> {
        let mut handle = ptr::null_mut();
        // SAFETY: pseudo thread/process handles are used only as API inputs;
        // each successful open yields an independently owned token handle.
        if unsafe { OpenThreadToken(GetCurrentThread(), TOKEN_QUERY, 1, &mut handle) } == 0 {
            if unsafe { GetLastError() } != ERROR_NO_TOKEN {
                return Err("ACL_TOKEN_QUERY");
            }
            if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut handle) } == 0 {
                return Err("ACL_TOKEN_QUERY");
            }
        }
        let token = Token(handle);
        let mut required = 0;
        // SAFETY: the sizing call has a null/zero buffer as specified by Win32.
        unsafe { GetTokenInformation(token.0, 1, ptr::null_mut(), 0, &mut required); }
        if required < mem::size_of::<SidAndAttributes>() as u32 || required > 4096 {
            return Err("ACL_TOKEN_SIZE");
        }
        let mut buffer = vec![0usize; (required as usize).div_ceil(mem::size_of::<usize>())];
        let mut returned = required;
        // SAFETY: the allocated aligned buffer is at least required bytes and
        // stays alive throughout extraction of TOKEN_USER's SID.
        if unsafe { GetTokenInformation(token.0, 1, buffer.as_mut_ptr().cast(), required, &mut returned) } == 0
            || returned > required || returned < mem::size_of::<SidAndAttributes>() as u32 {
            return Err("ACL_TOKEN_QUERY");
        }
        let header = unsafe { ptr::read_unaligned(buffer.as_ptr().cast::<SidAndAttributes>()) };
        let start = buffer.as_ptr() as usize;
        let end = start.checked_add(returned as usize).ok_or("ACL_TOKEN_SIZE")?;
        let sid_start = header.sid as usize;
        if sid_start < start || sid_start.checked_add(8).is_none_or(|value| value > end) {
            return Err("ACL_TOKEN_SID");
        }
        // SAFETY: the fixed SID header is within the returned TOKEN_USER buffer.
        let count = unsafe { *header.sid.cast::<u8>().add(1) } as usize;
        let length = 8usize.checked_add(count.checked_mul(4).ok_or("ACL_TOKEN_SID")?)
            .ok_or("ACL_TOKEN_SID")?;
        if count > 15 || sid_start.checked_add(length).is_none_or(|value| value > end)
            || unsafe { IsValidSid(header.sid) } == 0 {
            return Err("ACL_TOKEN_SID");
        }
        let mut sid = vec![0u32; length.div_ceil(4)];
        // SAFETY: the SID range was checked above and the destination owns at
        // least length bytes; no token-buffer pointers escape this function.
        unsafe { ptr::copy_nonoverlapping(header.sid.cast::<u8>(), sid.as_mut_ptr().cast::<u8>(), length); }
        Ok(Sid(sid))
    }
    fn trusted_installer() -> Option<Sid> {
        let account = wide("NT SERVICE\\TrustedInstaller");
        let mut size = 0;
        let mut domain_size = 0;
        let mut use_kind = 0;
        // SAFETY: sizing call; both output buffers are null and have zero size.
        unsafe { LookupAccountNameW(ptr::null(), account.as_ptr(), ptr::null_mut(), &mut size,
            ptr::null_mut(), &mut domain_size, &mut use_kind); }
        if !(8..=68).contains(&size) || domain_size > 4096 { return None; }
        let mut sid = vec![0u32; (size as usize).div_ceil(4)];
        let mut domain = vec![0u16; domain_size.max(1) as usize];
        // SAFETY: both initialized buffers meet the sizes returned by the first
        // call and stay alive. Lookup is an OS identity lookup, never a grant.
        if unsafe { LookupAccountNameW(ptr::null(), account.as_ptr(), sid.as_mut_ptr().cast(), &mut size,
            domain.as_mut_ptr(), &mut domain_size, &mut use_kind) } == 0
            || unsafe { IsValidSid(sid.as_ptr().cast()) } == 0 { return None; }
        Some(Sid(sid))
    }
    struct Trusted {
        user: Sid,
        system: LocalAllocation,
        administrators: LocalAllocation,
        installer: Option<Sid>,
    }
    impl Trusted {
        fn new() -> Result<Self> {
            Ok(Self { user: current_user()?, system: fixed_sid("S-1-5-18")?,
                administrators: fixed_sid("S-1-5-32-544")?, installer: trusted_installer() })
        }
        fn contains(&self, sid: *const c_void, ancestor: bool) -> bool {
            // SAFETY: every input SID was validated inside its owning native
            // descriptor/buffer, all of which stay alive across these calls.
            unsafe {
                EqualSid(sid, self.user.pointer()) != 0
                    || EqualSid(sid, self.system.0) != 0
                    || EqualSid(sid, self.administrators.0) != 0
                    || ancestor && self.installer.as_ref().is_some_and(|value| EqualSid(sid, value.pointer()) != 0)
            }
        }
    }

    fn shape(file: &File, directory: bool) -> Result<()> {
        let handle = file.as_raw_handle();
        let mut information = FileInformation::default();
        let mut flags = 0;
        // SAFETY: a borrowed File keeps the handle valid and the output structs
        // are writable with the exact Win32 layouts declared above.
        if unsafe { GetFileType(handle) } != 1
            || unsafe { GetFileInformationByHandle(handle, &mut information) } == 0 {
            return Err("ACL_FILE_KIND");
        }
        if information.attributes & REPARSE_ATTRIBUTE != 0 { return Err("ACL_REPARSE_REFUSED"); }
        if (information.attributes & DIRECTORY_ATTRIBUTE != 0) != directory
            || information.attributes & 0x40 != 0 { return Err("ACL_FILE_KIND"); }
        if !directory && information.links != 1 { return Err("ACL_HARDLINK_REFUSED"); }
        if unsafe { GetVolumeInformationByHandleW(handle, ptr::null_mut(), 0, ptr::null_mut(),
            ptr::null_mut(), &mut flags, ptr::null_mut(), 0) } == 0 || flags & 8 == 0 {
            return Err("ACL_PERSISTENT_VOLUME_REQUIRED");
        }
        Ok(())
    }

    fn check_security(file: &File, trusted: &Trusted, ancestor: bool) -> Result<()> {
        let mut owner = ptr::null_mut();
        let mut dacl = ptr::null_mut();
        let mut descriptor = ptr::null_mut();
        // SAFETY: File owns a valid handle opened with READ_CONTROL. Success
        // returns one LocalFree-owned descriptor and borrowed component pointers.
        if unsafe { GetSecurityInfo(file.as_raw_handle(), FILE_OBJECT, OWNER_INFORMATION | DACL_INFORMATION,
            &mut owner, ptr::null_mut(), &mut dacl, ptr::null_mut(), &mut descriptor) } != 0 {
            return Err("ACL_SECURITY_QUERY");
        }
        if descriptor.is_null() { return Err("ACL_SECURITY_QUERY"); }
        let _descriptor = LocalAllocation(descriptor);
        if owner.is_null() || unsafe { IsValidSecurityDescriptor(descriptor) } == 0
            || unsafe { IsValidSid(owner) } == 0 || !trusted.contains(owner, ancestor) {
            return Err("ACL_OWNER_REFUSED");
        }
        let mut present = 0;
        let mut defaulted = 0;
        if unsafe { GetSecurityDescriptorDacl(descriptor, &mut present, &mut dacl, &mut defaulted) } == 0
            || present == 0 || dacl.is_null() || unsafe { IsValidAcl(dacl) } == 0 {
            return Err("ACL_DACL_REQUIRED");
        }
        // SAFETY: IsValidAcl succeeded for this descriptor-owned ACL. Header
        // values and each returned ACE range are checked before further reads.
        let header = unsafe { ptr::read_unaligned(dacl) };
        let acl_start = dacl as usize;
        let acl_end = acl_start.checked_add(header.size as usize).ok_or("ACL_MALFORMED")?;
        if header.size < mem::size_of::<Acl>() as u16 { return Err("ACL_MALFORMED"); }
        for index in 0..u32::from(header.ace_count) {
            let mut ace = ptr::null_mut();
            if unsafe { GetAce(dacl, index, &mut ace) } == 0 || ace.is_null() { return Err("ACL_MALFORMED"); }
            let start = ace as usize;
            if start < acl_start || start.checked_add(4).is_none_or(|value| value > acl_end) { return Err("ACL_MALFORMED"); }
            let ace_header = unsafe { ptr::read_unaligned(ace.cast::<AceHeader>()) };
            if ace_header.size < 16 || start.checked_add(ace_header.size as usize).is_none_or(|value| value > acl_end)
                || ace_header.flags & !0x1f != 0 || !matches!(ace_header.kind, 0 | 1) { return Err("ACL_UNSUPPORTED_ACE"); }
            let mask = unsafe { ptr::read_unaligned(ace.cast::<u8>().add(4).cast::<u32>()) };
            let sid = unsafe { ace.cast::<u8>().add(8) };
            let count = unsafe { *sid.add(1) } as usize;
            let length = 8 + count * 4;
            if count > 15 || length > ace_header.size as usize - 8 || unsafe { IsValidSid(sid.cast()) } == 0
                || unsafe { GetLengthSid(sid.cast()) } as usize != length { return Err("ACL_MALFORMED"); }
            // Denials cannot expand authority. For ancestors INHERIT_ONLY does
            // not affect that opened ancestor; children are checked separately.
            if ace_header.kind == 1 || ancestor && ace_header.flags & INHERIT_ONLY_ACE != 0 { continue; }
            if mask != 0 && !trusted.contains(sid.cast(), ancestor)
                && (!ancestor || mask & !SAFE_ANCESTOR_RIGHTS != 0) { return Err("ACL_UNTRUSTED_ACCESS"); }
        }
        Ok(())
    }

    fn local_path(path: &Path) -> Result<()> {
        if !path.is_absolute() { return Err("ACL_ABSOLUTE_PATH_REQUIRED"); }
        let mut components = path.components();
        let drive = match components.next() {
            Some(Component::Prefix(prefix)) => match prefix.kind() {
                Prefix::Disk(drive) | Prefix::VerbatimDisk(drive) => drive,
                _ => return Err("ACL_LOCAL_DISK_REQUIRED"),
            },
            _ => return Err("ACL_LOCAL_DISK_REQUIRED"),
        };
        if components.clone().count() > 256 { return Err("ACL_PATH_SIZE"); }
        for component in components {
            match component {
                Component::RootDir => {}
                Component::Normal(value) => {
                    let text: Vec<u16> = value.encode_wide().collect();
                    if text.is_empty() || text.contains(&0) || text.contains(&u16::from(b':'))
                        || matches!(text.last(), Some(0x20 | 0x2e)) { return Err("ACL_PATH_COMPONENT"); }
                }
                _ => return Err("ACL_PATH_COMPONENT"),
            }
        }
        let root = wide(&format!("{}:\\", char::from(drive)));
        // SAFETY: terminated local drive-root string. Network/removable roots
        // are refused rather than claimed as this local fixed-disk boundary.
        if unsafe { GetDriveTypeW(root.as_ptr()) } != 3 { return Err("ACL_LOCAL_DISK_REQUIRED"); }
        Ok(())
    }

    fn open_guarded(path: &Path, private_root: Option<&Path>, append: bool) -> Result<GuardedFile> {
        local_path(path)?;
        let trusted = Trusted::new()?;
        let parent = path.parent().ok_or("ACL_PARENT_REQUIRED")?;
        let mut paths: Vec<PathBuf> = parent.ancestors().map(Path::to_path_buf).collect();
        paths.reverse();
        let mut directories = Vec::with_capacity(paths.len());
        for path in paths {
            let file = OpenOptions::new().read(true).access_mode(READ_CONTROL | FILE_READ_ATTRIBUTES)
                .share_mode(3).custom_flags(OPEN_REPARSE_POINT | BACKUP_SEMANTICS)
                .open(&path).map_err(|_| "ACL_PARENT_OPEN")?;
            shape(&file, true)?;
            check_security(&file, &trusted, private_root != Some(path.as_path()))?;
            directories.push(file);
        }
        let mut options = OpenOptions::new();
        options.read(true).share_mode(3).custom_flags(OPEN_REPARSE_POINT);
        if append { options.append(true).create(true); }
        let file = options.open(path).map_err(|error| {
            if error.raw_os_error() == Some(32) { "ACL_FILE_BUSY" } else { "ACL_FILE_OPEN" }
        })?;
        shape(&file, false)?;
        check_security(&file, &trusted, false)?;
        Ok(GuardedFile { file, _directories: directories })
    }
    pub fn validate_private_file_handle(file: &File) -> Result<()> {
        shape(file, false)?;
        check_security(file, &Trusted::new()?, false)
    }
    pub fn open_private_journal(root: &Path) -> Result<GuardedFile> {
        local_path(root)?;
        open_guarded(&root.join("inbox.jsonl"), Some(root), true)
    }
    pub fn read_private_config(path: &Path, limit: usize) -> Result<Vec<u8>> {
        let read_limit = limit.checked_add(1).ok_or("ACL_FILE_LIMIT")?;
        if limit == 0 { return Err("ACL_FILE_LIMIT"); }
        let mut guard = open_guarded(path, None, false)?;
        let file = guard.file_mut();
        if file.metadata().map_err(|_| "ACL_FILE_METADATA")?.len() > limit as u64 { return Err("ACL_FILE_LIMIT"); }
        let mut bytes = Vec::new();
        file.take(read_limit as u64).read_to_end(&mut bytes).map_err(|_| "ACL_FILE_READ")?;
        if bytes.len() > limit { return Err("ACL_FILE_LIMIT"); }
        Ok(bytes)
    }
}
