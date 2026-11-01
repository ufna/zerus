//! Native notification center, independent of Tauri windows and service transport.
//! Existing GUI delivery still owns policy; this module performs only SDK calls.
use std::ptr::NonNull;

use block2::RcBlock;
use objc2::{rc::Retained, runtime::Bool};
use objc2_foundation::{ns_string, NSArray, NSDictionary, NSError, NSString};
use objc2_user_notifications::{
    UNAuthorizationOptions, UNAuthorizationStatus, UNMutableNotificationContent,
    UNNotificationRequest, UNNotificationSettings, UNNotificationSound, UNUserNotificationCenter,
};
use zerus_client_core::attention;

use crate::notifier::{
    completion::{once, take},
    Center, Done,
};

fn describe(error: *mut NSError) -> Option<String> {
    // SAFETY: the system passes null or a valid error.
    unsafe { error.as_ref() }.map(|error| error.localizedDescription().to_string())
}

/// Preserve the SDK's Fn block while consuming the original continuation once.
/// Native copies share that same continuation; an uncalled final drop still
/// drops its armed accounting guard. This factory grants no native permission.
fn authorization_reply(done: Done<(bool, Option<String>)>) -> RcBlock<dyn Fn(Bool, *mut NSError)> {
    let done = once(done);
    RcBlock::new(move |granted: Bool, error: *mut NSError| {
        if let Some(done) = take(&done) {
            done((granted.as_bool(), describe(error)));
        }
    })
}

fn error_reply(done: Done<Option<String>>) -> RcBlock<dyn Fn(*mut NSError)> {
    let done = once(done);
    RcBlock::new(move |error: *mut NSError| {
        if let Some(done) = take(&done) {
            done(describe(error));
        }
    })
}

/// `UNUserNotificationCenter.currentNotificationCenter`, asked at each use.
pub(super) struct System;

impl Center for System {
    fn authorize(&self, done: Done<(bool, Option<String>)>) {
        let done = once(done);
        let block = RcBlock::new(move |settings: NonNull<UNNotificationSettings>| {
            let Some(done) = take(&done) else { return };
            // SAFETY: the system passes a valid settings object.
            let status = unsafe { settings.as_ref() }.authorizationStatus();
            if status == UNAuthorizationStatus::NotDetermined {
                let reply = authorization_reply(done);
                UNUserNotificationCenter::currentNotificationCenter()
                    .requestAuthorizationWithOptions_completionHandler(
                        UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound,
                        &reply,
                    );
            } else {
                done((
                    status == UNAuthorizationStatus::Authorized
                        || status == UNAuthorizationStatus::Provisional,
                    None,
                ));
            }
        });
        UNUserNotificationCenter::currentNotificationCenter()
            .getNotificationSettingsWithCompletionHandler(&block);
    }

    fn add(&self, token: &str, title: &str, body: &str, done: Done<Option<String>>) {
        let content = UNMutableNotificationContent::new();
        content.setTitle(&NSString::from_str(title));
        content.setBody(&NSString::from_str(body));
        content.setSound(Some(&UNNotificationSound::defaultSound()));
        let value = NSString::from_str(token);
        let info = NSDictionary::<NSString, NSString>::from_slices(
            &[ns_string!("hgsAttentionToken")],
            &[&*value],
        );
        // SAFETY: a dictionary of strings is a valid userInfo; the cast only
        // drops its element types.
        unsafe {
            let info: Retained<NSDictionary> = Retained::cast_unchecked(info);
            content.setUserInfo(&info);
        }
        let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
            &NSString::from_str(&attention::request_id(token)),
            &content,
            None,
        );
        let reply = error_reply(done);
        UNUserNotificationCenter::currentNotificationCenter()
            .addNotificationRequest_withCompletionHandler(&request, Some(&reply));
    }

    fn remove(&self, token: &str) {
        let center = UNUserNotificationCenter::currentNotificationCenter();
        let ids =
            NSArray::from_retained_slice(&[NSString::from_str(&attention::request_id(token))]);
        center.removePendingNotificationRequestsWithIdentifiers(&ids);
        center.removeDeliveredNotificationsWithIdentifiers(&ids);
    }
}

#[cfg(test)]
mod tests;
