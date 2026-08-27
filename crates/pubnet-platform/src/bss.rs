use crate::types::AuthMode;

const OUI_IEEE_80211: [u8; 3] = [0x00, 0x0f, 0xac];
const AKM_PSK: u8 = 2;
const AKM_FT_PSK: u8 = 4;
const AKM_FT_SAE: u8 = 6;
const AKM_SAE: u8 = 8;
const AKM_FT_SAE_EXT: u8 = 9;

/// Scan an IE byte slice (from a BSS entry's raw IEs) for the RSN IE (tag 0x30)
/// and return the auth mode it describes. Returns `AuthMode::Unknown` when the
/// slice contains no RSN IE or the IE is malformed.
pub fn parse_rsn_ie(ies: &[u8]) -> AuthMode {
    let mut i = 0;
    while i + 1 < ies.len() {
        let tag = ies[i];
        let len = ies[i + 1] as usize;
        let body_start = i + 2;
        let body_end = body_start + len;
        if body_end > ies.len() {
            break;
        }
        if tag == 0x30 {
            return parse_rsn_body(&ies[body_start..body_end]);
        }
        i = body_end;
    }
    AuthMode::Unknown
}

/// Parse the body of an RSN IE (after tag and length bytes) and classify the
/// AKM suites present. Malformed bodies return `AuthMode::Unknown`.
fn parse_rsn_body(body: &[u8]) -> AuthMode {
    // Minimum: 2 (version) + 4 (group cipher) + 2 (pairwise count) = 8 bytes
    if body.len() < 8 {
        return AuthMode::Unknown;
    }

    // Skip version [0-1] and group cipher suite [2-5]
    let pairwise_count = u16::from_le_bytes([body[6], body[7]]) as usize;
    // Clamp to avoid multiplication overflow on adversarial input
    let pairwise_count = pairwise_count.min(64);

    let akm_offset = 8 + pairwise_count * 4;
    if akm_offset + 2 > body.len() {
        return AuthMode::Unknown;
    }

    let akm_count = u16::from_le_bytes([body[akm_offset], body[akm_offset + 1]]) as usize;
    let akm_count = akm_count.min(64);

    let akm_list_start = akm_offset + 2;
    if akm_list_start + akm_count * 4 > body.len() {
        return AuthMode::Unknown;
    }

    let mut has_psk = false;
    let mut has_sae = false;

    for k in 0..akm_count {
        let off = akm_list_start + k * 4;
        let oui = &body[off..off + 3];
        let suite_type = body[off + 3];
        if oui != OUI_IEEE_80211 {
            continue;
        }
        match suite_type {
            AKM_PSK | AKM_FT_PSK => has_psk = true,
            AKM_SAE | AKM_FT_SAE | AKM_FT_SAE_EXT => has_sae = true,
            _ => {}
        }
    }

    match (has_psk, has_sae) {
        (true, true) => AuthMode::SaeTransition,
        (false, true) => AuthMode::Sae,
        (true, false) => AuthMode::Psk,
        (false, false) => AuthMode::Unknown,
    }
}

// ---------------------------------------------------------------------------
// Tests — pure function over hand-crafted byte fixtures. No hardware needed.
// spec: wifi-auth-protocol-detection#S1, #S2
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // Minimal RSN IE body for a given AKM suite list.
    // Layout: version(2) + group(4) + pairwise_count(2) + pairwise(4) + akm_count(2) + akms(4*n)
    fn rsn_body(akm_types: &[u8]) -> Vec<u8> {
        let mut body: Vec<u8> = vec![
            0x01, 0x00, // version = 1
            0x00, 0x0f, 0xac, 0x04, // group cipher: CCMP
            0x01, 0x00, // pairwise count = 1
            0x00, 0x0f, 0xac, 0x04, // pairwise: CCMP
        ];
        body.push(akm_types.len() as u8);
        body.push(0x00); // AKM count low byte
        for &t in akm_types {
            body.extend_from_slice(&[0x00, 0x0f, 0xac, t]);
        }
        body
    }

    fn make_ie(body: &[u8]) -> Vec<u8> {
        let mut ie = vec![0x30, body.len() as u8];
        ie.extend_from_slice(body);
        ie
    }

    #[test]
    fn pure_wpa2_psk() {
        // spec: wifi-auth-protocol-detection#S1
        let body = rsn_body(&[AKM_PSK]);
        assert_eq!(parse_rsn_ie(&make_ie(&body)), AuthMode::Psk);
    }

    #[test]
    fn pure_wpa3_sae() {
        // spec: wifi-auth-protocol-detection#S1
        let body = rsn_body(&[AKM_SAE]);
        assert_eq!(parse_rsn_ie(&make_ie(&body)), AuthMode::Sae);
    }

    #[test]
    fn transition_mode_psk_and_sae() {
        // spec: wifi-auth-protocol-detection#S2
        let body = rsn_body(&[AKM_PSK, AKM_SAE]);
        assert_eq!(parse_rsn_ie(&make_ie(&body)), AuthMode::SaeTransition);
    }

    #[test]
    fn transition_mode_ft_psk_and_sae() {
        let body = rsn_body(&[AKM_FT_PSK, AKM_SAE]);
        assert_eq!(parse_rsn_ie(&make_ie(&body)), AuthMode::SaeTransition);
    }

    #[test]
    fn transition_mode_psk_and_ft_sae() {
        let body = rsn_body(&[AKM_PSK, AKM_FT_SAE]);
        assert_eq!(parse_rsn_ie(&make_ie(&body)), AuthMode::SaeTransition);
    }

    #[test]
    fn ft_sae_only_is_sae() {
        let body = rsn_body(&[AKM_FT_SAE]);
        assert_eq!(parse_rsn_ie(&make_ie(&body)), AuthMode::Sae);
    }

    #[test]
    fn no_rsn_ie_is_unknown() {
        // Only a vendor IE (tag 0xDD), no RSN IE
        let ies = vec![0xdd, 0x04, 0x00, 0x50, 0xf2, 0x01];
        assert_eq!(parse_rsn_ie(&ies), AuthMode::Unknown);
    }

    #[test]
    fn rsn_ie_preceded_by_other_ies() {
        let mut ies = vec![0x00, 0x04, b't', b'e', b's', b't']; // SSID IE
        let body = rsn_body(&[AKM_SAE]);
        ies.extend_from_slice(&make_ie(&body));
        assert_eq!(parse_rsn_ie(&ies), AuthMode::Sae);
    }

    #[test]
    fn truncated_body_is_unknown() {
        // Tag 0x30, length 3, but only 2 body bytes — malformed
        let ies = vec![0x30, 0x03, 0x01, 0x00];
        assert_eq!(parse_rsn_ie(&ies), AuthMode::Unknown);
    }

    #[test]
    fn empty_slice_is_unknown() {
        assert_eq!(parse_rsn_ie(&[]), AuthMode::Unknown);
    }

    #[test]
    fn body_too_short_is_unknown() {
        // RSN body length = 5, needs at least 8
        let ies = vec![0x30, 0x05, 0x01, 0x00, 0x00, 0x0f, 0xac];
        assert_eq!(parse_rsn_ie(&ies), AuthMode::Unknown);
    }

    #[test]
    fn unknown_oui_is_unknown() {
        let body = vec![
            0x01, 0x00, // version
            0x00, 0x0f, 0xac, 0x04, // group
            0x01, 0x00, // pairwise count = 1
            0x00, 0x0f, 0xac, 0x04, // pairwise
            0x01, 0x00, // AKM count = 1
            0x00, 0x50, 0xf2, 0x02, // Microsoft OUI, not IEEE 802.11
        ];
        assert_eq!(parse_rsn_ie(&make_ie(&body)), AuthMode::Unknown);
    }
}
