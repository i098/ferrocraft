use std::fs::File;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::Path;
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{INVALID_HANDLE_VALUE, GENERIC_READ, GENERIC_WRITE};
use windows_sys::Win32::Security::Authorization::{SetNamedSecurityInfoW, SE_FILE_OBJECT};
use windows_sys::Win32::Security::{
    AddAccessAllowedAceEx, GetTokenInformation, InitializeAcl, InitializeSecurityDescriptor,
    SetSecurityDescriptorControl, SetSecurityDescriptorDacl, TokenUser, ACCESS_ALLOWED_ACE, ACL,
    ACL_REVISION, CONTAINER_INHERIT_ACE, DACL_SECURITY_INFORMATION, OBJECT_INHERIT_ACE,
    PROTECTED_DACL_SECURITY_INFORMATION, SECURITY_ATTRIBUTES, SECURITY_DESCRIPTOR,
    SECURITY_MAX_SID_SIZE, SE_DACL_PROTECTED, TOKEN_QUERY, TOKEN_USER,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, GetVolumeInformationByHandleW, CREATE_NEW, FILE_ALL_ACCESS, FILE_ATTRIBUTE_NORMAL,
    FILE_PERSISTENT_ACLS, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

#[repr(C)]
struct User {
    info: TOKEN_USER,
    sid: [u8; SECURITY_MAX_SID_SIZE as usize],
}

#[repr(C)]
struct UserAcl {
    acl: ACL,
    ace: ACCESS_ALLOWED_ACE,
    sid: [u8; SECURITY_MAX_SID_SIZE as usize],
}

fn user_acl(flags: u32) -> io::Result<UserAcl> {
    unsafe {
        let mut token = null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return Err(io::Error::last_os_error());
        }
        let token = OwnedHandle::from_raw_handle(token);
        let mut user: User = std::mem::zeroed();
        let mut len = 0;
        if GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            (&mut user as *mut User).cast(),
            size_of::<User>() as u32,
            &mut len,
        ) == 0
        {
            return Err(io::Error::last_os_error());
        }
        let mut acl: UserAcl = std::mem::zeroed();
        let raw = (&mut acl as *mut UserAcl).cast();
        if InitializeAcl(raw, size_of::<UserAcl>() as u32, ACL_REVISION) == 0
            || AddAccessAllowedAceEx(raw, ACL_REVISION, flags, FILE_ALL_ACCESS, user.info.User.Sid) == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(acl)
    }
}

pub(super) fn create(dir: &Path) -> io::Result<tempfile::NamedTempFile> {
    let acl = user_acl(0)?;
    let mut descriptor: SECURITY_DESCRIPTOR = unsafe { std::mem::zeroed() };
    let sd = (&mut descriptor as *mut SECURITY_DESCRIPTOR).cast();
    let attributes = unsafe {
        if InitializeSecurityDescriptor(sd, 1) == 0
            || SetSecurityDescriptorDacl(sd, 1, (&acl as *const UserAcl).cast(), 0) == 0
            || SetSecurityDescriptorControl(sd, SE_DACL_PROTECTED, SE_DACL_PROTECTED) == 0
        {
            return Err(io::Error::last_os_error());
        }
        SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: sd,
            bInheritHandle: 0,
        }
    };
    let staged = tempfile::Builder::new().make_in(dir.canonicalize()?, |path| {
        let path: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
        unsafe {
            let handle = CreateFileW(
                path.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                &attributes,
                CREATE_NEW,
                FILE_ATTRIBUTE_NORMAL,
                null_mut(),
            );
            if handle == INVALID_HANDLE_VALUE {
                Err(io::Error::last_os_error())
            } else {
                Ok(File::from_raw_handle(handle))
            }
        }
    })?;
    let mut flags = 0;
    if unsafe {
        GetVolumeInformationByHandleW(
            staged.as_file().as_raw_handle(),
            null_mut(),
            0,
            null_mut(),
            null_mut(),
            &mut flags,
            null_mut(),
            0,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    if flags & FILE_PERSISTENT_ACLS == 0 {
        return Err(io::Error::other("Mod staging requires a volume with persistent ACLs"));
    }
    Ok(staged)
}

pub(super) fn restrict_directory(path: &Path) -> io::Result<()> {
    let acl = user_acl(CONTAINER_INHERIT_ACE | OBJECT_INHERIT_ACE)?;
    let path: Vec<u16> = path.canonicalize()?.as_os_str().encode_wide().chain([0]).collect();
    let status = unsafe {
        SetNamedSecurityInfoW(
            path.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            null_mut(),
            null_mut(),
            (&acl as *const UserAcl).cast(),
            null(),
        )
    };
    if status == 0 {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(status as i32))
    }
}
