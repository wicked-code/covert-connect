use anyhow::Result;
use windows::{
    core::BSTR,
    Win32::{
        Foundation::{RPC_E_CHANGED_MODE, VARIANT_BOOL},
        NetworkManagement::WindowsFirewall::{
            INetFwPolicy2, INetFwRule, NetFwPolicy2, NetFwRule, NET_FW_ACTION_ALLOW, NET_FW_IP_PROTOCOL_TCP,
            NET_FW_PROFILE2_ALL, NET_FW_RULE_DIR_IN,
        },
        System::Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED},
    },
};

struct ComGuard;

impl Drop for ComGuard {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

pub fn fix_firewall() {
    if let Err(error) = fix_firewall_rule() {
        tracing::warn!("failed to configure Windows Firewall rule: {error:?}");
    }
}

fn fix_firewall_rule() -> Result<()> {
    let exe_path = std::env::current_exe()?.canonicalize()?.to_string_lossy().into_owned();
    let exe_path = exe_path.strip_prefix(r"\\?\").unwrap_or(&exe_path);
    let rule_name = format!("cc-tun ({exe_path})");

    let com_result = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    if com_result.is_err() && com_result != RPC_E_CHANGED_MODE {
        com_result.ok()?;
    }
    let _com_guard = com_result.is_ok().then_some(ComGuard);

    unsafe {
        let policy: INetFwPolicy2 = CoCreateInstance(&NetFwPolicy2, None, CLSCTX_ALL)?;
        let rules = policy.Rules()?;
        let rule_name_bstr = BSTR::from(rule_name.as_str());

        if rules.Item(&rule_name_bstr).is_ok() {
            return Ok(());
        }

        let rule: INetFwRule = CoCreateInstance(&NetFwRule, None, CLSCTX_ALL)?;

        rule.SetName(&BSTR::from(rule_name.as_str()))?;
        rule.SetApplicationName(&BSTR::from(exe_path))?;
        rule.SetEnabled(VARIANT_BOOL::from(true))?;
        rule.SetProtocol(NET_FW_IP_PROTOCOL_TCP.0)?;
        rule.SetDirection(NET_FW_RULE_DIR_IN)?;
        rule.SetAction(NET_FW_ACTION_ALLOW)?;
        rule.SetProfiles(NET_FW_PROFILE2_ALL.0)?;

        rules.Add(&rule)?;
    }

    Ok(())
}
