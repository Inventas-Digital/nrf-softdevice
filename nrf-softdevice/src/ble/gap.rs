use crate::ble::*;
use crate::util::get_union_field;
use crate::{raw, RawError};

pub(crate) unsafe fn on_evt(ble_evt: *const raw::ble_evt_t) {
    let gap_evt = get_union_field(ble_evt, &(*ble_evt).evt.gap_evt);
    match (*ble_evt).header.evt_id as u32 {
        raw::BLE_GAP_EVTS_BLE_GAP_EVT_CONNECTED => {
            let params = &gap_evt.params.connected;

            debug!(
                "conn_params conn_sup_timeout={:?} max_conn_interval={:?} min_conn_interval={:?} slave_latency={:?}",
                params.conn_params.conn_sup_timeout,
                params.conn_params.max_conn_interval,
                params.conn_params.min_conn_interval,
                params.conn_params.slave_latency,
            );

            let handled = match Role::from_raw(params.role) {
                #[cfg(feature = "ble-central")]
                Role::Central => central::CONNECT_PORTAL.call(ble_evt),
                #[cfg(feature = "ble-peripheral")]
                Role::Peripheral => peripheral::ADV_PORTAL.call(ble_evt),
            };
            if !handled {
                raw::sd_ble_gap_disconnect(gap_evt.conn_handle, raw::BLE_HCI_REMOTE_USER_TERMINATED_CONNECTION as _);
            }
        }
        raw::BLE_GAP_EVTS_BLE_GAP_EVT_DISCONNECTED => {
            trace!("on_disconnected conn_handle={:?}", gap_evt.conn_handle);
            connection::with_state_by_conn_handle(gap_evt.conn_handle, |state| state.on_disconnected(ble_evt));
        }
        raw::BLE_GAP_EVTS_BLE_GAP_EVT_CONN_PARAM_UPDATE => {
            let conn_params = gap_evt.params.conn_param_update.conn_params;

            debug!(
                "on_conn_param_update conn_handle={:?} conn_sup_timeout={:?} max_conn_interval={:?} min_conn_interval={:?} slave_latency={:?}",
                gap_evt.conn_handle,
                conn_params.conn_sup_timeout,
                conn_params.max_conn_interval,
                conn_params.min_conn_interval,
                conn_params.slave_latency,
            );

            connection::with_state_by_conn_handle(gap_evt.conn_handle, |state| {
                state.conn_params = conn_params;
            });
        }
        #[cfg(feature = "ble-central")]
        raw::BLE_GAP_EVTS_BLE_GAP_EVT_CONN_PARAM_UPDATE_REQUEST => {
            let conn_handle = gap_evt.conn_handle;
            let conn_params = gap_evt.params.conn_param_update_request.conn_params;
            debug!(
                "on_conn_param_update_request conn_handle={:?} conn_sup_timeout={:?} max_conn_interval={:?} min_conn_interval={:?} slave_latency={:?}",
                gap_evt.conn_handle,
                conn_params.conn_sup_timeout,
                conn_params.max_conn_interval,
                conn_params.min_conn_interval,
                conn_params.slave_latency,
            );

            let ret = raw::sd_ble_gap_conn_param_update(conn_handle, &conn_params);
            if let Err(err) = RawError::convert(ret) {
                warn!("sd_ble_gap_conn_param_update err {:?}", err);
            }
        }
        raw::BLE_GAP_EVTS_BLE_GAP_EVT_TIMEOUT => {
            trace!("on_timeout conn_handle={:?}", gap_evt.conn_handle);

            let params = &gap_evt.params.timeout;
            match params.src as u32 {
                #[cfg(feature = "ble-central")]
                raw::BLE_GAP_TIMEOUT_SRC_CONN => central::CONNECT_PORTAL.call(ble_evt),
                #[cfg(feature = "ble-central")]
                raw::BLE_GAP_TIMEOUT_SRC_SCAN => central::SCAN_PORTAL.call(ble_evt),
                x => panic!("unknown timeout src {:?}", x),
            };
        }
        #[cfg(feature = "ble-peripheral")]
        raw::BLE_GAP_EVTS_BLE_GAP_EVT_ADV_SET_TERMINATED => {
            trace!("adv_set_termnated");
            peripheral::ADV_PORTAL.call(ble_evt);
        }
        #[cfg(feature = "ble-central")]
        raw::BLE_GAP_EVTS_BLE_GAP_EVT_ADV_REPORT => {
            trace!("central on_adv_report");
            central::SCAN_PORTAL.call(ble_evt);
        }
        raw::BLE_GAP_EVTS_BLE_GAP_EVT_PHY_UPDATE_REQUEST => {
            let peer_preferred_phys = gap_evt.params.phy_update_request.peer_preferred_phys;
            let conn_handle = gap_evt.conn_handle;

            trace!(
                "on_phy_update_request conn_handle={:?} rx_phys={:?} tx_phys={:?}",
                conn_handle,
                peer_preferred_phys.rx_phys,
                peer_preferred_phys.tx_phys
            );

            let phys = raw::ble_gap_phys_t {
                rx_phys: peer_preferred_phys.rx_phys,
                tx_phys: peer_preferred_phys.tx_phys,
            };

            let ret = raw::sd_ble_gap_phy_update(conn_handle, &phys as *const raw::ble_gap_phys_t);

            if let Err(_err) = RawError::convert(ret) {
                warn!("sd_ble_gap_phy_update err {:?}", _err);
            }
        }
        raw::BLE_GAP_EVTS_BLE_GAP_EVT_PHY_UPDATE => {
            let _phy_update = gap_evt.params.phy_update;

            trace!(
                "on_phy_update conn_handle={:?} status={:?} rx_phy={:?} tx_phy={:?}",
                gap_evt.conn_handle,
                _phy_update.status,
                _phy_update.rx_phy,
                _phy_update.tx_phy
            );
        }
        #[cfg(any(feature = "s113", feature = "s132", feature = "s140"))]
        raw::BLE_GAP_EVTS_BLE_GAP_EVT_DATA_LENGTH_UPDATE_REQUEST => {
            let _peer_params = gap_evt.params.data_length_update_request.peer_params;

            trace!(
                "on_data_length_update_request conn_handle={:?} max_rx_octets={:?} max_rx_time_us={:?} max_tx_octets={:?} max_tx_time_us={:?}",
                gap_evt.conn_handle,
                _peer_params.max_rx_octets,
                _peer_params.max_rx_time_us,
                _peer_params.max_tx_octets,
                _peer_params.max_tx_time_us,
            );

            let conn_handle = gap_evt.conn_handle;
            if let Some(mut conn) = Connection::from_handle(conn_handle) {
                let _ = conn.data_length_update(None);
            }
        }
        #[cfg(any(feature = "s113", feature = "s132", feature = "s140"))]
        raw::BLE_GAP_EVTS_BLE_GAP_EVT_DATA_LENGTH_UPDATE => {
            let effective_params = gap_evt.params.data_length_update.effective_params;

            connection::with_state_by_conn_handle(gap_evt.conn_handle, |state| {
                state.data_length_effective = effective_params.max_tx_octets as u8;
            });

            debug!(
                "on_data_length_update conn_handle={:?} max_rx_octets={:?} max_rx_time_us={:?} max_tx_octets={:?} max_tx_time_us={:?}",
                gap_evt.conn_handle,
                effective_params.max_rx_octets,
                effective_params.max_rx_time_us,
                effective_params.max_tx_octets,
                effective_params.max_tx_time_us,
            );
        }
        #[cfg(feature = "ble-rssi")]
        raw::BLE_GAP_EVTS_BLE_GAP_EVT_RSSI_CHANGED => {
            let new_rssi = gap_evt.params.rssi_changed.rssi;
            connection::with_state_by_conn_handle(gap_evt.conn_handle, |state| {
                state.rssi = match state.rssi {
                    None => Some(new_rssi),
                    Some(old_rssi) => Some((((old_rssi as i16) * 7 + (new_rssi as i16)) / 8) as i8),
                };
            });
        }
        raw::BLE_GAP_EVTS_BLE_GAP_EVT_SEC_PARAMS_REQUEST => {
            let params = &gap_evt.params.sec_params_request;
            let peer_params = params.peer_params;
            trace!("ble evt sec params request conn={:x}, bond={:?}, io_caps={:?}, keypress={:?}, lesc={:?}, mitm={:?}, oob={:?}, key_size={}..={}",
                    gap_evt.conn_handle, peer_params.bond(), peer_params.io_caps(), peer_params.keypress(), peer_params.lesc(), peer_params.mitm(), peer_params.oob(),
                    peer_params.min_key_size, peer_params.max_key_size);

            if let Some(conn) = Connection::from_handle(gap_evt.conn_handle) {
                let reply = conn.with_state(|state| {
                    #[cfg(feature = "ble-sec")]
                    {
                        let own_pk = state.security.handler.and_then(|h| h.lesc_public_key(&conn));
                        state.security.lesc = own_pk.is_some();
                        state.security.own_pk.pk = own_pk.unwrap_or([0; 64]);
                    }

                    #[cfg(not(feature = "ble-peripheral"))]
                    let sec_params = None;
                    #[cfg(feature = "ble-peripheral")]
                    let sec_params = if state.role == Role::Peripheral {
                        #[cfg(not(feature = "ble-sec"))]
                        let sec_params = default_security_params();
                        #[cfg(feature = "ble-sec")]
                        let sec_params = state
                            .security
                            .handler
                            .map(|h| h.security_params(&conn))
                            .unwrap_or_else(default_security_params);

                        // In the central role the peer's parameters aren't known yet, so
                        // legacy pairing is caught in BLE_GAP_EVT_AUTH_STATUS instead.
                        #[cfg(feature = "ble-sec")]
                        if peer_params.lesc() == 0 && state.security.handler.is_some_and(|h| h.require_lesc(&conn)) {
                            return None;
                        }

                        Some(sec_params)
                    } else {
                        None
                    };

                    #[allow(unused_mut)]
                    let mut keyset = state.keyset();

                    // The LTK isn't distributed with LE Secure Connections, and the softdevice
                    // requires the peer's encryption key pointer to be NULL then.
                    #[cfg(all(feature = "ble-peripheral", feature = "ble-sec"))]
                    if state.role == Role::Peripheral && state.security.lesc && peer_params.lesc() != 0 {
                        keyset.keys_peer.p_enc_key = core::ptr::null_mut();
                    }

                    Some((sec_params, keyset))
                });

                let ret = match reply {
                    Some((sec_params, keyset)) => raw::sd_ble_gap_sec_params_reply(
                        gap_evt.conn_handle,
                        raw::BLE_GAP_SEC_STATUS_SUCCESS as u8,
                        sec_params.as_ref().map(|x| x as *const _).unwrap_or(core::ptr::null()),
                        &keyset,
                    ),
                    None => {
                        warn!("Rejecting pairing: peer doesn't support LE Secure Connections");
                        raw::sd_ble_gap_sec_params_reply(
                            gap_evt.conn_handle,
                            raw::BLE_GAP_SEC_STATUS_PAIRING_NOT_SUPP as u8,
                            core::ptr::null(),
                            core::ptr::null(),
                        )
                    }
                };

                if let Err(_err) = RawError::convert(ret) {
                    warn!("sd_ble_gap_sec_params_reply err {:?}", _err);
                }
            } else {
                warn!("Received SEC_PARAMS_REQUEST with an invalid connection handle");
            }
        }
        raw::BLE_GAP_EVTS_BLE_GAP_EVT_PASSKEY_DISPLAY => {
            let params = &gap_evt.params.passkey_display;
            debug_assert_eq!(params.match_request(), 0);
            trace!(
                "on_passkey_display passkey={}",
                core::str::from_utf8_unchecked(&params.passkey)
            );
            #[cfg(feature = "ble-sec")]
            connection::with_state_by_conn_handle(gap_evt.conn_handle, |state| {
                if let Some(handler) = state.security.handler {
                    handler.display_passkey(&params.passkey)
                }
            });
        }
        raw::BLE_GAP_EVTS_BLE_GAP_EVT_AUTH_KEY_REQUEST => {
            let params = &gap_evt.params.auth_key_request;
            trace!("on_auth_key_request key_type={}", params.key_type);

            #[cfg(not(feature = "ble-sec"))]
            let handled = false;
            #[cfg(feature = "ble-sec")]
            let handled = connection::with_state_by_conn_handle(gap_evt.conn_handle, |state| {
                state
                    .security
                    .handler
                    .and_then(|handler| match u32::from(params.key_type) {
                        raw::BLE_GAP_AUTH_KEY_TYPE_PASSKEY => Connection::from_handle(gap_evt.conn_handle)
                            .map(|conn| handler.enter_passkey(PasskeyReply::new(conn))),
                        raw::BLE_GAP_AUTH_KEY_TYPE_OOB => Connection::from_handle(gap_evt.conn_handle)
                            .map(|conn| handler.recv_out_of_band(OutOfBandReply::new(conn))),
                        _ => None,
                    })
            })
            .is_some();

            if !handled {
                let ret = raw::sd_ble_gap_auth_key_reply(
                    gap_evt.conn_handle,
                    raw::BLE_GAP_AUTH_KEY_TYPE_NONE as u8,
                    core::ptr::null(),
                );

                if let Err(_err) = RawError::convert(ret) {
                    warn!("sd_ble_gap_auth_key_reply err {:?}", _err);
                }
            }
        }
        #[cfg(feature = "ble-peripheral")]
        raw::BLE_GAP_EVTS_BLE_GAP_EVT_SEC_INFO_REQUEST => {
            let params = &gap_evt.params.sec_info_request;
            trace!("ble evt sec info request: enc_info={}, id_info={}, sign_info={}, master_id: {{ ediv: {:x}, rand: {:?} }}, peer_addr: {{ addr: {:?}, addr_id_peer: {}, addr_type: {} }}",
                params.enc_info(), params.id_info(), params.sign_info(), params.master_id.ediv, params.master_id.rand,
                params.peer_addr.addr, params.peer_addr.addr_id_peer(), params.peer_addr.addr_type());

            #[cfg(feature = "ble-sec")]
            let key = Connection::from_handle(gap_evt.conn_handle).and_then(|conn| {
                conn.security_handler().and_then(|x| {
                    x.get_key(&conn, MasterId::from_raw(params.master_id))
                        .filter(|key| key.as_raw().lesc() != 0 || !x.require_lesc(&conn))
                })
            });

            #[cfg(not(feature = "ble-sec"))]
            let key_ptr = core::ptr::null();
            #[cfg(feature = "ble-sec")]
            let key_ptr = key
                .as_ref()
                .map(|x| x.as_raw() as *const _)
                .unwrap_or(core::ptr::null());

            let ret =
                raw::sd_ble_gap_sec_info_reply(gap_evt.conn_handle, key_ptr, core::ptr::null(), core::ptr::null());

            if let Err(_err) = RawError::convert(ret) {
                warn!("sd_ble_gap_sec_info_reply err {:?}", _err);
            }
        }
        raw::BLE_GAP_EVTS_BLE_GAP_EVT_CONN_SEC_UPDATE => {
            let params = &gap_evt.params.conn_sec_update;
            trace!(
                "ble evt conn sec update sec_mode=({},{}), encr_key_size={}",
                params.conn_sec.sec_mode.sm(),
                params.conn_sec.sec_mode.lv(),
                params.conn_sec.encr_key_size
            );
            if let Some(conn) = Connection::from_handle(gap_evt.conn_handle) {
                conn.with_state(|state| {
                    state.security_mode = SecurityMode::try_from_raw(params.conn_sec.sec_mode).unwrap_or_default();
                    #[cfg(feature = "ble-sec")]
                    if let Some(handler) = state.security.handler {
                        handler.on_security_update(&conn, state.security_mode);
                    }
                });
            }
        }
        raw::BLE_GAP_EVTS_BLE_GAP_EVT_AUTH_STATUS => {
            let params = &gap_evt.params.auth_status;
            trace!(
                "ble evt auth status: bonded={}, error_src={}, lesc={}, kdist_own={}, kdist_peer={}",
                params.bonded(),
                params.error_src(),
                params.lesc(),
                params.kdist_own._bitfield_1.get(0, 8),
                params.kdist_peer._bitfield_1.get(0, 8)
            );
            #[cfg(feature = "ble-sec")]
            if u32::from(params.auth_status) == raw::BLE_GAP_SEC_STATUS_SUCCESS {
                if let Some(conn) = Connection::from_handle(gap_evt.conn_handle) {
                    conn.with_state(|state| {
                        if let Some(handler) = state.security.handler {
                            if params.lesc() == 0 && handler.require_lesc(&conn) {
                                warn!("Disconnecting: pairing completed without LE Secure Connections");
                                let _ = state.disconnect();
                                return;
                            }

                            if params.bonded() == 0 {
                                return;
                            }

                            let peer_id = if params.kdist_peer.id() != 0 {
                                IdentityKey::from_raw(state.security.peer_id)
                            } else {
                                debug!("Peer identity key not distributed; falling back to address");
                                IdentityKey::from_addr(state.peer_address)
                            };

                            // With LE Secure Connections both sides derive the LTK themselves
                            // instead of distributing it, so it's in the own keys for both roles.
                            let enc_key = match state.role {
                                #[cfg(feature = "ble-central")]
                                Role::Central if params.lesc() != 0 => &state.security.own_enc_key,
                                #[cfg(feature = "ble-central")]
                                Role::Central => &state.security.peer_enc_key,
                                #[cfg(feature = "ble-peripheral")]
                                Role::Peripheral => &state.security.own_enc_key,
                            };

                            handler.on_bonded(
                                &conn,
                                MasterId::from_raw(enc_key.master_id),
                                EncryptionInfo::from_raw(enc_key.enc_info),
                                peer_id,
                            );
                        }
                    });
                }
            }
        }
        #[cfg(feature = "ble-central")]
        raw::BLE_GAP_EVTS_BLE_GAP_EVT_SEC_REQUEST => {
            let params = &gap_evt.params.sec_request;
            trace!(
                "ble evt auth status: bond={}, mitm={}, lesc={}, keypress={}",
                params.bond(),
                params.mitm(),
                params.lesc(),
                params.keypress(),
            );
            if let Some(conn) = Connection::from_handle(gap_evt.conn_handle) {
                #[cfg(feature = "ble-sec")]
                let res = match conn.encrypt() {
                    Ok(()) => Ok(()),
                    Err(EncryptError::NoSecurityHandler) | Err(EncryptError::PeerKeysNotFound) => {
                        conn.request_pairing()
                    }
                    Err(EncryptError::Disconnected) => Err(AuthenticateError::Disconnected),
                    Err(EncryptError::Raw(err)) => Err(AuthenticateError::Raw(err)),
                };
                #[cfg(not(feature = "ble-sec"))]
                let res = conn.request_pairing();
                if let Err(err) = res {
                    warn!("Failed to initiate encryption {:?}", err);
                }
            }
        }
        #[cfg(feature = "ble-sec")]
        raw::BLE_GAP_EVTS_BLE_GAP_EVT_LESC_DHKEY_REQUEST => {
            let params = &gap_evt.params.lesc_dhkey_request;
            trace!("ble evt lesc dhkey request conn={:x}", gap_evt.conn_handle);

            if let Some(conn) = Connection::from_handle(gap_evt.conn_handle) {
                let dhkey = conn
                    .security_handler()
                    .and_then(|h| h.lesc_dhkey(&conn, &(*params.p_pk_peer).pk));

                let ret = match dhkey {
                    Some(key) => {
                        raw::sd_ble_gap_lesc_dhkey_reply(gap_evt.conn_handle, &raw::ble_gap_lesc_dhkey_t { key })
                    }
                    // Never answer with a predictable DHKey: with an invalid peer key that
                    // would let the peer complete pairing with a key it can compute.
                    None => {
                        warn!("LESC DHKey not available; disconnecting");
                        let _ = conn.disconnect();
                        raw::NRF_SUCCESS
                    }
                };

                if let Err(_err) = RawError::convert(ret) {
                    warn!("sd_ble_gap_lesc_dhkey_reply err {:?}", _err);
                    let _ = conn.disconnect();
                }
            } else {
                warn!("Received LESC_DHKEY_REQUEST with an invalid connection handle");
            }
        }
        // BLE_GAP_EVTS_BLE_GAP_EVT_KEY_PRESSED (LESC central pairing)
        // BLE_GAP_EVTS_BLE_GAP_EVT_RSSI_CHANGED
        // BLE_GAP_EVTS_BLE_GAP_EVT_SCAN_REQ_REPORT
        // BLE_GAP_EVTS_BLE_GAP_EVT_QOS_CHANNEL_SURVEY_REPORT
        _ => {}
    }
}

pub fn set_device_identities_list(
    sd: &Softdevice,
    id_keys: &[IdentityKey],
    local_irks: Option<&[IdentityResolutionKey]>,
) -> Result<(), RawError> {
    let _ = sd;
    const MAX_LEN: usize = raw::BLE_GAP_DEVICE_IDENTITIES_MAX_COUNT as usize;
    assert!(id_keys.len() <= MAX_LEN);
    assert!(local_irks.map(|x| x.len() == id_keys.len()).unwrap_or(true));

    let mut p_id_keys: [*const raw::ble_gap_id_key_t; MAX_LEN] = [core::ptr::null(); MAX_LEN];
    let pp_id_keys = if !id_keys.is_empty() {
        for (a, b) in id_keys.iter().zip(p_id_keys.iter_mut()) {
            *b = a.as_raw() as *const _;
        }
        Some(&p_id_keys[..id_keys.len()])
    } else {
        None
    };

    let mut p_local_irks: [*const raw::ble_gap_irk_t; MAX_LEN] = [core::ptr::null(); MAX_LEN];
    let pp_local_irks = if let Some(local_irks) = local_irks {
        for (a, b) in local_irks.iter().zip(p_local_irks.iter_mut()) {
            *b = a.as_raw() as *const _;
        }
        Some(&p_local_irks[..local_irks.len()])
    } else {
        None
    };

    let ret = unsafe {
        raw::sd_ble_gap_device_identities_set(
            pp_id_keys.map(|x| x.as_ptr()).unwrap_or(core::ptr::null()),
            pp_local_irks.map(|x| x.as_ptr()).unwrap_or(core::ptr::null()),
            id_keys.len() as u8,
        )
    };
    RawError::convert(ret)
}

pub fn set_whitelist(sd: &Softdevice, addrs: &[Address]) -> Result<(), RawError> {
    let _ = sd;
    const MAX_LEN: usize = raw::BLE_GAP_WHITELIST_ADDR_MAX_COUNT as usize;
    assert!(addrs.len() <= MAX_LEN);

    let mut p_addrs: [*const raw::ble_gap_addr_t; MAX_LEN] = [core::ptr::null(); MAX_LEN];
    let pp_addrs = if !addrs.is_empty() {
        for (a, b) in addrs.iter().zip(p_addrs.iter_mut()) {
            *b = a.as_raw() as *const _;
        }
        Some(&p_addrs[..addrs.len()])
    } else {
        None
    };

    let ret = unsafe {
        raw::sd_ble_gap_whitelist_set(
            pp_addrs.map(|x| x.as_ptr()).unwrap_or(core::ptr::null()),
            addrs.len() as u8,
        )
    };
    RawError::convert(ret)
}

pub fn default_security_params() -> raw::ble_gap_sec_params_t {
    let mut sec_params: raw::ble_gap_sec_params_t = unsafe { core::mem::zeroed() };

    sec_params.min_key_size = 7;
    sec_params.max_key_size = 16;

    sec_params.set_io_caps(raw::BLE_GAP_IO_CAPS_NONE as u8);
    sec_params
}
