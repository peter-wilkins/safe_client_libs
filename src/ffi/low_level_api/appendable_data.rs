// Copyright 2016 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under (1) the MaidSafe.net Commercial License,
// version 1.0 or later, or (2) The General Public License (GPL), version 3, depending on which
// licence you accepted on initial access to the Software (the "Licences").
//
// By contributing code to the SAFE Network Software, or to this project generally, you agree to be
// bound by the terms of the MaidSafe Contributor Agreement, version 1.0.  This, along with the
// Licenses can be found in the root directory of this project at LICENSE, COPYING and CONTRIBUTOR.
//
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied.
//
// Please review the Licences for the specific language governing permissions and limitations
// relating to use of the SAFE Network Software.

use core::errors::CoreError;
use ffi::app::App;
use ffi::helper;
use ffi::low_level_api::{AppendableDataHandle, DataIdHandle, EncryptKeyHandle, SignKeyHandle};
use ffi::low_level_api::object_cache::object_cache;
use routing::{Data, Filter, PrivAppendableData, PubAppendableData, XOR_NAME_LEN, XorName};
use std::iter;
use std::ptr;

/// Wrapper for PrivAppendableData and PubAppendableData.
#[derive(Clone)]
pub enum AppendableData {
    /// Public appendable data.
    Pub(PubAppendableData),
    /// Private appendable data.
    Priv(PrivAppendableData),
}

impl AppendableData {
    fn filter_mut(&mut self) -> &mut Filter {
        match *self {
            AppendableData::Pub(ref mut data) => &mut data.filter,
            AppendableData::Priv(ref mut data) => &mut data.filter,
        }
    }
}

impl Into<Data> for AppendableData {
    fn into(self) -> Data {
        match self {
            AppendableData::Pub(data) => Data::PubAppendable(data),
            AppendableData::Priv(data) => Data::PrivAppendable(data),
        }
    }
}

/// Create new PubAppendableData
#[no_mangle]
pub unsafe extern "C" fn appendable_data_new_pub(app: *const App,
                                                 name: *const [u8; XOR_NAME_LEN],
                                                 o_handle: *mut AppendableDataHandle)
                                                 -> i32 {
    helper::catch_unwind_i32(|| {
        let client = (*app).get_client();
        let name = XorName(*name);

        let (owner_key, sign_key) = {
            let client = unwrap!(client.lock());
            let owner_key = *ffi_try!(client.get_public_signing_key());
            let sign_key = ffi_try!(client.get_secret_signing_key()).clone();
            (owner_key, sign_key)
        };

        let data = PubAppendableData::new(name,
                                          0,
                                          vec![owner_key],
                                          vec![],
                                          Filter::black_list(iter::empty()),
                                          Some(&sign_key));
        let data = AppendableData::Pub(ffi_try!(data.map_err(CoreError::from)));
        let handle = unwrap!(object_cache().lock()).insert_appendable_data(data);

        ptr::write(o_handle, handle);
        0
    })
}

/// Create new PrivAppendableData
#[no_mangle]
pub unsafe extern "C" fn appendable_data_new_priv(app: *const App,
                                                  name: *const [u8; XOR_NAME_LEN],
                                                  encrypt_key_h: EncryptKeyHandle,
                                                  o_handle: *mut AppendableDataHandle)
                                                  -> i32 {
    helper::catch_unwind_i32(|| {
        let mut object_cache = unwrap!(object_cache().lock());

        let client = (*app).get_client();
        let name = XorName(*name);

        let (owner_key, sign_key) = {
            let client = unwrap!(client.lock());
            let owner_key = *ffi_try!(client.get_public_signing_key());
            let sign_key = ffi_try!(client.get_secret_signing_key()).clone();
            (owner_key, sign_key)
        };
        let encrypt_key = *ffi_try!(object_cache.get_encrypt_key(encrypt_key_h));

        let data = PrivAppendableData::new(name,
                                           0,
                                           vec![owner_key],
                                           vec![],
                                           Filter::black_list(iter::empty()),
                                           encrypt_key,
                                           Some(&sign_key));
        let data = AppendableData::Priv(ffi_try!(data.map_err(CoreError::from)));
        let handle = object_cache.insert_appendable_data(data);

        ptr::write(o_handle, handle);
        0
    })
}

/// Get existing appendable data from Network.
#[no_mangle]
pub unsafe extern "C" fn appendable_data_get(app: *const App,
                                             data_id_h: DataIdHandle,
                                             o_handle: *mut AppendableDataHandle)
                                             -> i32 {
    helper::catch_unwind_i32(|| {
        let data_id = *ffi_try!(unwrap!(object_cache().lock()).get_data_id(data_id_h));

        let client = (*app).get_client();
        let resp_getter = ffi_try!(unwrap!(client.lock()).get(data_id, None));
        let data = match ffi_try!(resp_getter.get()) {
            Data::PubAppendable(data) => AppendableData::Pub(data),
            Data::PrivAppendable(data) => AppendableData::Priv(data),
            _ => ffi_try!(Err(CoreError::ReceivedUnexpectedData)),
        };

        let handle = unwrap!(object_cache().lock()).insert_appendable_data(data);

        ptr::write(o_handle, handle);
        0
    })
}

/// PUT appendable data.
#[no_mangle]
pub unsafe extern "C" fn appendable_data_put(app: *const App,
                                             appendable_data_h: AppendableDataHandle)
                                             -> i32 {
    helper::catch_unwind_i32(|| {
        let data = {
            let mut object_cache = unwrap!(object_cache().lock());
            ffi_try!(object_cache.get_appendable_data(appendable_data_h)).clone()
        };

        let client = (*app).get_client();
        let resp_getter = ffi_try!(unwrap!(client.lock()).put(data.into(), None));
        ffi_try!(resp_getter.get());

        0
    })
}

// TODO Need to clone delete_data too - ask routing to provide that
/// POST appendable data (bumps the version).
#[no_mangle]
pub unsafe extern "C" fn appendable_data_post(app: *const App,
                                              appendable_data_h: AppendableDataHandle)
                                              -> i32 {
    helper::catch_unwind_i32(|| {
        let client = (*app).get_client();

        let new_ad = {
            let sign_key = ffi_try!(unwrap!(client.lock()).get_secret_signing_key()).clone();
            let mut object_cache = unwrap!(object_cache().lock());
            let ad = ffi_try!(object_cache.get_appendable_data(appendable_data_h));

            match *ad {
                AppendableData::Pub(ref old_data) => {
                    let new_data =
                        ffi_try!(PubAppendableData::new(old_data.name,
                                                        old_data.version + 1,
                                                        old_data.current_owner_keys.clone(),
                                                        old_data.previous_owner_keys.clone(),
                                                        old_data.filter.clone(),
                                                        Some(&sign_key))
                            .map_err(CoreError::from));
                    AppendableData::Pub(new_data)
                }
                AppendableData::Priv(ref old_data) => {
                    let new_data =
                        ffi_try!(PrivAppendableData::new(old_data.name,
                                                         old_data.version + 1,
                                                         old_data.current_owner_keys.clone(),
                                                         old_data.previous_owner_keys.clone(),
                                                         old_data.filter.clone(),
                                                         old_data.encrypt_key.clone(),
                                                         Some(&sign_key))
                            .map_err(CoreError::from));
                    AppendableData::Priv(new_data)
                }
            }
        };
        let resp_getter = ffi_try!(unwrap!(client.lock()).post(new_ad.clone().into(), None));
        ffi_try!(resp_getter.get());
        let _ = unwrap!(object_cache().lock()).appendable_data.insert(appendable_data_h, new_ad);

        0
    })
}

// TODO: DELETE (disabled for now)

/// Switch the filter of the appendable data.
#[no_mangle]
pub unsafe extern "C" fn appendable_data_toggle_filter(appendable_data_h: AppendableDataHandle)
                                                       -> i32 {
    helper::catch_unwind_i32(|| {
        let mut object_cache = unwrap!(object_cache().lock());
        let ad = ffi_try!(object_cache.get_appendable_data(appendable_data_h));

        let filter = ad.filter_mut();
        match *filter {
            Filter::BlackList(_) => *filter = Filter::white_list(iter::empty()),
            Filter::WhiteList(_) => *filter = Filter::black_list(iter::empty()),
        }

        0
    })
}

/// Insert a new entry to the (whitelist or blacklist) filter. If the key was
/// already present in the filter, this is a no-op.
#[no_mangle]
pub extern "C" fn appendable_data_insert_to_filter(appendable_data_h: AppendableDataHandle,
                                                   sign_key_h: SignKeyHandle)
                                                   -> i32 {
    helper::catch_unwind_i32(|| {
        let mut object_cache = unwrap!(object_cache().lock());
        let sign_key = *ffi_try!(object_cache.get_sign_key(sign_key_h));
        let ad = ffi_try!(object_cache.get_appendable_data(appendable_data_h));

        let _ = match *ad.filter_mut() {
            Filter::WhiteList(ref mut list) |
            Filter::BlackList(ref mut list) => list.insert(sign_key),
        };

        0
    })
}

/// Remove the given key from the (whitelist or blacklist) filter. If the key
/// isn't present in the filter, this is a no-op.
#[no_mangle]
pub extern "C" fn appendable_data_remove_from_filter(appendable_data_h: AppendableDataHandle,
                                                     sign_key_h: SignKeyHandle)
                                                     -> i32 {
    helper::catch_unwind_i32(|| {
        let mut object_cache = unwrap!(object_cache().lock());
        let sign_key = *ffi_try!(object_cache.get_sign_key(sign_key_h));
        let ad = ffi_try!(object_cache.get_appendable_data(appendable_data_h));

        let _ = match *ad.filter_mut() {
            Filter::WhiteList(ref mut list) |
            Filter::BlackList(ref mut list) => list.remove(&sign_key),
        };

        0
    })
}

/// Get number of appended data items.
#[no_mangle]
pub unsafe extern "C" fn appendable_data_num_of_data(appendable_data_h: AppendableDataHandle,
                                                     o_num: *mut u64)
                                                     -> i32 {
    helper::catch_unwind_i32(|| {
        let mut object_cache = unwrap!(object_cache().lock());
        let ad = ffi_try!(object_cache.get_appendable_data(appendable_data_h));
        let num = match *ad {
            AppendableData::Pub(ref data) => data.data.len(),
            AppendableData::Priv(ref data) => data.data.len(),
        };

        ptr::write(o_num, num as u64);
        0
    })
}
