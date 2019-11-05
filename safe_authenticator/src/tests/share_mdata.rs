// Copyright 2018 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under The General Public License (GPL), version 3.
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied. Please review the Licences for the specific language governing
// permissions and limitations relating to use of the SAFE Network Software.

use safe_core::ffi::errors::{AuthError, ERR_INVALID_OWNER, ERR_NO_SUCH_DATA};
use crate::ffi::apps::*;
use crate::ffi::ipc::encode_share_mdata_resp;
use crate::run;
use crate::test_utils::{self, Payload};
use ffi_utils::test_utils::{call_1, call_vec};
use futures::Future;
use maidsafe_utilities::serialisation::serialise;
use safe_core::ipc::req::AppExchangeInfo;
use safe_core::ipc::resp::{AppAccess, UserMetadata, METADATA_KEY};
use safe_core::ipc::{self, AuthReq, IpcError, IpcMsg, IpcReq, IpcResp, ShareMData, ShareMDataReq};
use safe_core::Client;
use safe_nd::{MDataAction, MDataPermissionSet, MDataSeqValue, PublicKey, SeqMutableData};
use std::collections::BTreeMap;

// Test making an empty request to share mutable data.
#[test]
fn share_zero_mdatas() {
    let authenticator = test_utils::create_account_and_login();

    let msg = IpcMsg::Req {
        req_id: ipc::gen_req_id(),
        req: IpcReq::ShareMData(ShareMDataReq {
            app: test_utils::rand_app(),
            mdata: vec![],
        }),
    };
    let encoded_msg = unwrap!(ipc::encode_msg(&msg));

    let decoded = unwrap!(test_utils::auth_decode_ipc_msg_helper(
        &authenticator,
        &encoded_msg
    ));
    match decoded {
        (
            IpcMsg::Req {
                req: IpcReq::ShareMData(ShareMDataReq { mdata, .. }),
                ..
            },
            Some(Payload::Metadata(metadatas)),
        ) => {
            assert_eq!(mdata.len(), 0);
            assert_eq!(metadatas.len(), 0);
        }
        _ => panic!("Unexpected: {:?}", decoded),
    };
}

// Test making a request to share mutable data with barebones mdata.
#[test]
fn share_some_mdatas() {
    let authenticator = test_utils::create_account_and_login();

    let user = unwrap!(run(&authenticator, move |client| {
        ok!(client.public_key())
    }));

    const NUM_MDATAS: usize = 3;

    let mut mdatas = Vec::new();
    let mut metadatas = Vec::new();

    for _ in 0..NUM_MDATAS {
        let name = new_rand::random();
        let tag = 0;
        let mdata = {
            SeqMutableData::new_with_data(name, tag, Default::default(), Default::default(), user)
        };

        unwrap!(run(&authenticator, move |client| {
            client
                .put_seq_mutable_data(mdata)
                .map_err(AuthError::CoreError)
        }));

        mdatas.push(ShareMData {
            type_tag: tag,
            name,
            perms: MDataPermissionSet::new().allow(MDataAction::Insert),
        });
        metadatas.push((None, name, tag));
    }

    let msg = IpcMsg::Req {
        req_id: ipc::gen_req_id(),
        req: IpcReq::ShareMData(ShareMDataReq {
            app: test_utils::rand_app(),
            mdata: mdatas.clone(),
        }),
    };
    let encoded_msg = unwrap!(ipc::encode_msg(&msg));

    let decoded = unwrap!(test_utils::auth_decode_ipc_msg_helper(
        &authenticator,
        &encoded_msg
    ));
    match decoded {
        (
            IpcMsg::Req {
                req: IpcReq::ShareMData(ShareMDataReq { mdata, .. }),
                ..
            },
            Some(Payload::Metadata(received_metadatas)),
        ) => {
            assert_eq!(mdata, mdatas);
            assert_eq!(received_metadatas, metadatas);
        }
        _ => panic!("Unexpected: {:?}", decoded),
    };
}

// Test making a request to share invalid mutable data.
#[test]
fn share_invalid_mdatas() {
    test_utils::init_log();

    let authenticator = test_utils::create_account_and_login();

    const NUM_MDATAS: usize = 3;
    let mut share_mdatas = Vec::new();

    for _ in 0..NUM_MDATAS {
        let name = new_rand::random();
        let tag = 15_000;

        share_mdatas.push(ShareMData {
            type_tag: tag,
            name,
            perms: MDataPermissionSet::new().allow(MDataAction::Insert),
        });
    }

    let msg = IpcMsg::Req {
        req_id: ipc::gen_req_id(),
        req: IpcReq::ShareMData(ShareMDataReq {
            app: test_utils::rand_app(),
            mdata: share_mdatas.clone(),
        }),
    };
    let encoded_msg = unwrap!(ipc::encode_msg(&msg));

    match test_utils::auth_decode_ipc_msg_helper(&authenticator, &encoded_msg) {
        Err((ERR_NO_SUCH_DATA, None)) => (),
        x => panic!("Unexpected result: {:?}", x),
    }
}

// Test making a request to share mdata with valid metadata.
#[test]
fn share_some_mdatas_with_valid_metadata() {
    let authenticator = test_utils::create_account_and_login();

    let app_id = test_utils::rand_app();
    let auth_req = AuthReq {
        app: app_id.clone(),
        app_container: false,
        app_permissions: Default::default(),
        containers: Default::default(),
    };

    let app_auth = unwrap!(test_utils::register_app(&authenticator, &auth_req));
    let app_key = PublicKey::from(app_auth.app_keys.bls_pk);

    let user = unwrap!(run(&authenticator, move |client| {
        ok!(client.public_key())
    }));

    const NUM_MDATAS: usize = 3;

    let perms = MDataPermissionSet::new().allow(MDataAction::Insert);
    let mut mdatas = Vec::new();
    let mut metadatas = Vec::new();
    for i in 0..NUM_MDATAS {
        let metadata = UserMetadata {
            name: Some(format!("name {}", i)),
            description: Some(format!("description {}", i)),
        };

        let name = new_rand::random();
        let tag = 10_000;
        let mdata = {
            let value = MDataSeqValue {
                data: unwrap!(serialise(&metadata)),
                version: 0,
            };
            let entries = btree_map![METADATA_KEY.to_vec() => value];
            SeqMutableData::new_with_data(name, tag, entries, BTreeMap::new(), user)
        };

        unwrap!(run(&authenticator, move |client| {
            client
                .put_seq_mutable_data(mdata)
                .map_err(AuthError::CoreError)
        }));

        mdatas.push(ShareMData {
            type_tag: tag,
            name,
            perms: perms.clone(),
        });
        metadatas.push((Some(metadata), name, tag));
    }

    let req_id = ipc::gen_req_id();
    let req = ShareMDataReq {
        app: app_id,
        mdata: mdatas.clone(),
    };
    let msg = IpcMsg::Req {
        req_id,
        req: IpcReq::ShareMData(req.clone()),
    };
    let encoded_msg = unwrap!(ipc::encode_msg(&msg));

    let decoded = unwrap!(test_utils::auth_decode_ipc_msg_helper(
        &authenticator,
        &encoded_msg
    ));
    match decoded {
        (
            IpcMsg::Req {
                req: IpcReq::ShareMData(ShareMDataReq { mdata, .. }),
                ..
            },
            Some(Payload::Metadata(received_metadatas)),
        ) => {
            assert_eq!(mdata, mdatas);
            assert_eq!(received_metadatas, metadatas);
        }
        _ => panic!("Unexpected: {:?}", decoded),
    };

    let req_c = unwrap!(req.into_repr_c());

    let _share_mdata_resp: String = unsafe {
        unwrap!(call_1(|ud, cb| encode_share_mdata_resp(
            &authenticator,
            &req_c,
            req_id,
            true,
            ud,
            cb,
        )))
    };

    for share_mdata in &mdatas {
        let name = share_mdata.name;
        let type_tag = share_mdata.type_tag;
        let mdata = unwrap!(run(&authenticator, move |client| {
            client
                .get_seq_mdata(name, type_tag)
                .map_err(AuthError::CoreError)
        }));
        let permissions = unwrap!(mdata.user_permissions(app_key));
        assert_eq!(permissions, &perms);
    }
}

// Test making a request to share mdata with invalid owners.
// FIXME: Fix this test when we implement multiple owners
#[test]
fn share_some_mdatas_with_ownership_error() {
    let authenticator = test_utils::create_account_and_login();

    let user = unwrap!(run(&authenticator, move |client| {
        ok!(client.public_key())
    }));

    let name = new_rand::random();
    let mdata = SeqMutableData::new_with_data(name, 0, btree_map![], btree_map![], user);

    unwrap!(run(&authenticator, move |client| {
        client
            .put_seq_mutable_data(mdata)
            .map_err(AuthError::CoreError)
    }));

    let share_md = ShareMData {
        type_tag: 0,
        name,
        perms: MDataPermissionSet::new().allow(MDataAction::Insert),
    };

    let req_id = ipc::gen_req_id();
    let req = ShareMDataReq {
        app: test_utils::rand_app(),
        mdata: vec![share_md.clone()],
    };
    let msg = IpcMsg::Req {
        req_id,
        req: IpcReq::ShareMData(req.clone()),
    };
    let encoded_msg = unwrap!(ipc::encode_msg(&msg));

    match test_utils::auth_decode_ipc_msg_helper(&authenticator, &encoded_msg) {
        Ok(..) => (),
        Err(err) => {
            assert_eq!(err, (ERR_INVALID_OWNER, None));
        }
    };

    let req_c = unwrap!(req.into_repr_c());

    let share_mdata_resp: String = unsafe {
        unwrap!(call_1(|ud, cb| encode_share_mdata_resp(
            &authenticator,
            &req_c,
            req_id,
            false,
            ud,
            cb,
        )))
    };

    match ipc::decode_msg(&share_mdata_resp) {
        Ok(IpcMsg::Resp {
            resp: IpcResp::ShareMData(Err(IpcError::ShareMDataDenied)),
            ..
        }) => (),
        x => panic!("Unexpected {:?}", x),
    };
}

// Test cases for:
// 1. Shared access is requested for an MData object that has metadata.
// a. Test that `name` and `description` were returned correctly. (This should mostly be covered by
// various tests in `safe_authenticator::tests` such as `share_some_mdatas()`.) Check that the
// returned `xor_name` and `type_tag` can identify the right MData.
// b. Add some tests similar to 2b and 2c (below), but where the MData does have metadata. The
// behavior should be the same.
// 2. Shared access is requested for an MData object that doesn't have metadata.
// a. Test that null was returned for the MD name and description, but that the returned `xor_name`
// and `type_tag` can correctly identify the right MData.
// b. Test that we can get the apps accessing the MData with `auth_apps_accessing_mutable_data`.
// Namely, each `AppAccess` object should contain all the correct information.
// c. If an app is listed in the MD permissions list, but is not listed in the registered apps list
// in Authenticator, then test that the `app_id` and `name` fields are null, but the public sign key
// and the list of permissions are correct.
#[test]
fn auth_apps_accessing_mdatas() {
    test_utils::init_log();
    let authenticator = test_utils::create_account_and_login();

    let user = unwrap!(run(&authenticator, move |client| {
        ok!(client.public_key())
    }));

    const NUM_MDATAS: usize = 3;
    const NUM_MDATAS_NO_META: usize = 3;

    // Create a few MData objects with metadata
    let perms = MDataPermissionSet::new().allow(MDataAction::Insert);
    let mut mdatas = Vec::new();
    let mut metadatas = Vec::new();
    let unregistered = PublicKey::from(threshold_crypto::SecretKey::random().public_key());

    for i in 0..(NUM_MDATAS + NUM_MDATAS_NO_META) {
        let metadata = if i < NUM_MDATAS {
            Some(UserMetadata {
                name: Some(format!("name {}", i)),
                description: Some(format!("description {}", i)),
            })
        } else {
            None
        };

        let name = new_rand::random();
        let tag = 10_000 + i as u64;
        let mdata = {
            let owners = user;

            // We need to test both with and without metadata
            let entries = match metadata {
                Some(ref meta) => {
                    let value = MDataSeqValue {
                        data: unwrap!(serialise(&meta)),
                        version: 0,
                    };
                    btree_map![METADATA_KEY.to_vec() => value]
                }
                None => btree_map![],
            };

            // Include one app in the permissions list that is not registered
            SeqMutableData::new_with_data(
                name,
                tag,
                entries,
                btree_map![unregistered => perms.clone()],
                owners,
            )
        };

        unwrap!(run(&authenticator, move |client| {
            client
                .put_seq_mutable_data(mdata)
                .map_err(AuthError::CoreError)
        }));

        mdatas.push(ShareMData {
            type_tag: tag,
            name,
            perms: perms.clone(),
        });
        metadatas.push((metadata, name, tag));
    }

    const NUM_APPS: usize = 3;

    let mut apps: Vec<(PublicKey, AppExchangeInfo)> = Vec::with_capacity(NUM_APPS);
    for _ in 0..NUM_APPS {
        // Create an app and register it.
        let app_id = test_utils::rand_app();
        let auth_req = AuthReq {
            app: app_id.clone(),
            app_container: false,
            app_permissions: Default::default(),
            containers: Default::default(),
        };

        let app_auth = unwrap!(test_utils::register_app(&authenticator, &auth_req));
        let app_key = PublicKey::from(app_auth.app_keys.bls_pk);

        // Share the Mdatas with the app.
        let req_id = ipc::gen_req_id();
        let req = ShareMDataReq {
            app: app_id.clone(),
            mdata: mdatas.clone(),
        };
        let msg = IpcMsg::Req {
            req_id,
            req: IpcReq::ShareMData(req.clone()),
        };
        let encoded_msg = unwrap!(ipc::encode_msg(&msg));

        let decoded = unwrap!(test_utils::auth_decode_ipc_msg_helper(
            &authenticator,
            &encoded_msg
        ));

        match decoded {
            (
                IpcMsg::Req {
                    req: IpcReq::ShareMData(ShareMDataReq { mdata, .. }),
                    ..
                },
                Some(Payload::Metadata(received_metadatas)),
            ) => {
                assert_eq!(mdata, mdatas);
                // Ensure the received metadatas, xor names and type tags are equal.
                // For mdata without metadata, received metadata should be `None`.
                assert_eq!(received_metadatas, metadatas);
            }
            _ => panic!("Unexpected: {:?}", decoded),
        };

        let req_c = unwrap!(req.into_repr_c());

        let _share_mdata_resp: String = unsafe {
            unwrap!(call_1(|ud, cb| {
                encode_share_mdata_resp(
                    &authenticator,
                    &req_c,
                    req_id,
                    true, // is_granted
                    ud,
                    cb,
                )
            }))
        };

        apps.push((app_key, app_id));
    }

    // Test the correctness of returned `AppAccess` objects
    for (_, name, tag) in metadatas {
        let app_access: Vec<AppAccess> = unsafe {
            unwrap!(call_vec(|ud, cb| auth_apps_accessing_mutable_data(
                &authenticator,
                &name.0,
                tag,
                ud,
                cb
            )))
        };

        // Check each accessing app
        for &(ref app_key, ref app_id) in &apps {
            let access = match app_access
                .iter()
                .find(|&access| access.sign_key == *app_key)
            {
                Some(access) => access,
                None => panic!("App not found in AppAccess list."),
            };

            assert_eq!(access.permissions, perms);
            assert_eq!(access.name, Some(app_id.name.clone()));
            assert_eq!(access.app_id, Some(app_id.id.clone()));
        }

        // Check unregistered app
        let access = match app_access
            .iter()
            .find(|&access| access.sign_key == unregistered)
        {
            Some(access) => access,
            None => panic!("Unregistered app not found in AppAccess list."),
        };

        assert_eq!(access.permissions, perms);
        assert_eq!(access.name, None);
        assert_eq!(access.app_id, None);
    }
}
