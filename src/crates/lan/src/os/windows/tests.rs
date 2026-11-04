use super::acl::{current_user, sid_string, Local};
use super::*;
use std::{fs, os::windows::ffi::OsStrExt, path::Path};
use windows_sys::Win32::{
    NetworkManagement::IpHelper::IP_ADAPTER_UNICAST_ADDRESS_LH,
    Networking::WinSock::SOCKET_ADDRESS,
    Security::{
        Authorization::{GetNamedSecurityInfoW, SE_FILE_OBJECT},
        GetAce, GetSecurityDescriptorControl, ACCESS_ALLOWED_ACE, DACL_SECURITY_INFORMATION,
        SE_DACL_PROTECTED,
    },
};

#[test]
fn adapter_addresses_exclude_loopback_and_prefer_physical_lan_interfaces() {
    let mut sockets = [
        [10, 0, 0, 8],
        [192, 168, 1, 9],
        [127, 0, 0, 1],
        [10, 0, 0, 8],
        [100, 69, 88, 16],
    ]
    .map(|bytes| {
        let mut socket = SOCKADDR_IN {
            sin_family: AF_INET,
            ..Default::default()
        };
        socket.sin_addr.S_un.S_addr = u32::from_ne_bytes(bytes);
        socket
    });
    let mut unicast: Vec<_> = sockets
        .iter_mut()
        .map(|socket| IP_ADAPTER_UNICAST_ADDRESS_LH {
            Address: SOCKET_ADDRESS {
                lpSockaddr: (socket as *mut SOCKADDR_IN).cast(),
                iSockaddrLength: size_of::<SOCKADDR_IN>() as i32,
            },
            ..Default::default()
        })
        .collect();
    let mut adapters: Vec<_> = unicast
        .iter_mut()
        .enumerate()
        .map(|(index, address)| IP_ADAPTER_ADDRESSES_LH {
            FirstUnicastAddress: address,
            OperStatus: IfOperStatusUp,
            IfType: if index == 1 { IF_TYPE_IEEE80211 } else { 131 }, // tunnel
            ..Default::default()
        })
        .collect();
    for index in 0..adapters.len() - 1 {
        adapters[index].Next = &mut adapters[index + 1];
    }
    let ips = |rows: Vec<((bool, u32), Ipv4Addr)>| {
        rows.into_iter()
            .map(|(_, ip)| ip.to_string())
            .collect::<Vec<_>>()
    };
    let rows = unsafe { collect_addresses(adapters.as_ptr()) };
    assert_eq!(
        ips(rows.clone()),
        ["192.168.1.9", "10.0.0.8", "100.69.88.16"]
    );
    assert_eq!(overlay_only(rows), ["100.69.88.16"]);
    adapters[1].OperStatus = 2; // IfOperStatusDown
    assert_eq!(
        ips(unsafe { collect_addresses(adapters.as_ptr()) }),
        ["10.0.0.8", "100.69.88.16"]
    );
}

fn access_entries(path: &Path) -> (u16, Vec<String>) {
    let path: Vec<_> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut descriptor = std::ptr::null_mut();
    let mut dacl = std::ptr::null_mut();
    assert_eq!(
        unsafe {
            GetNamedSecurityInfoW(
                path.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut dacl,
                std::ptr::null_mut(),
                &mut descriptor,
            )
        },
        ERROR_SUCCESS
    );
    let descriptor = Local(descriptor);
    let mut control = 0;
    let mut revision = 0;
    assert_ne!(
        unsafe { GetSecurityDescriptorControl(descriptor.0, &mut control, &mut revision) },
        0
    );
    assert!(!dacl.is_null());
    let mut entries = Vec::new();
    for index in 0..unsafe { (*dacl).AceCount } {
        let mut ace = std::ptr::null_mut();
        assert_ne!(unsafe { GetAce(dacl, u32::from(index), &mut ace) }, 0);
        let ace = unsafe { &*ace.cast::<ACCESS_ALLOWED_ACE>() };
        assert_eq!(ace.Header.AceType, 0); // ACCESS_ALLOWED_ACE_TYPE
        entries
            .push(unsafe { sid_string((&ace.SidStart as *const u32).cast_mut().cast()) }.unwrap());
    }
    entries.sort();
    // Windows can split a generic inheritable grant into effective and
    // inherit-only ACEs. Compare the granted identities, not the ACE count.
    entries.dedup();
    (control, entries)
}

#[test]
fn private_directory_protects_its_acl_and_new_files_inherit_it() {
    struct Fixture(std::path::PathBuf);
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let root = Fixture(std::env::temp_dir().join(format!("agenthud-acl-{}", uuid::Uuid::new_v4())));
    // A child store can be initialized before its parents are used as
    // stores. Every newly created directory must have TokenUser ownership
    // even in an elevated process; existing parents are left untouched.
    let nested = root.0.join("data").join("messaging");
    create_directory(&nested).unwrap();
    protect_directory(&nested).unwrap();
    crate::fs::private_dir(nested.parent().unwrap()).unwrap();
    crate::fs::private_dir(&root.0).unwrap();
    let (control, entries) = access_entries(&root.0);
    let mut expected = vec![current_user().unwrap(), "S-1-5-18".into()];
    expected.sort();
    assert_ne!(control & SE_DACL_PROTECTED, 0);
    assert_eq!(entries, expected);
    let file = root.0.join("clients.json");
    crate::fs::atomic_json(&file, &serde_json::json!({"first":true})).unwrap();
    crate::fs::atomic_json(&file, &serde_json::json!({"second":true})).unwrap();
    assert_eq!(access_entries(&file).1, expected);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&fs::read(&file).unwrap()).unwrap(),
        serde_json::json!({"second":true})
    );
}
