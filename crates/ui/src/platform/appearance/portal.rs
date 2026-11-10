// SPDX-License-Identifier: GPL-3.0-or-later

//! Linux: the system's light/dark preference from the XDG desktop portal (ADR-0038 S2).
//!
//! # The protocol
//! The desktop portal (`xdg-desktop-portal`, present on every modern Linux desktop and inside
//! Flatpak) is a D-Bus service on the *session* bus. Its `org.freedesktop.portal.Settings`
//! interface, at `/org/freedesktop/portal/desktop` of `org.freedesktop.portal.Desktop`, has:
//!
//! - `ReadOne(namespace s, key s) → v`: one setting. We read `org.freedesktop.appearance` /
//!   `color-scheme`, a `u32`: **0** no preference, **1** prefer dark, **2** prefer light.
//! - `Read(namespace s, key s) → v`: the same, deprecated, from portals older than version 2 of
//!   the interface (2023), with the value wrapped in **one more variant** (`v` inside `v`). We call
//!   it only when `ReadOne` is unknown.
//! - the signal `SettingChanged(namespace s, key s, value v)`, sent at every change of any
//!   setting. On the owner's desktop each change arrived three times; the [`Reporter`] drops the
//!   repeats.
//!
//! # Mapping (ADR-0038 S4, owner's choice)
//! 1 → dark; 2 → light; **0 → light**, because GNOME stores its default (light) style as
//! "default", reported as 0, and a GNOME user who chose light must not get dark. Any other number
//! is not documented: it counts as "no answer" (`None`, dark by ADR-0037 T9) and is recorded.
//!
//! # Order of operations, and why
//! 1. Connect to the session bus.
//! 2. **Subscribe first**, then read: a change that happens between the read and the subscription
//!    would otherwise be lost until the next change. Subscribed first, at worst we receive a
//!    change we have already read, which the reporter drops as a repeat.
//! 3. Read the current value (with the `Read` fallback) and report it: this is the answer the
//!    caller of `observe` is waiting for (at most 250 ms, ADR-0038 S5).
//! 4. Forward each `SettingChanged` for our key until the stream ends.
//!
//! # Threading and cost
//! All of it runs on one thread, `orchestra-appearance`, driven by `zbus::block_on` (the
//! `async-io` executor, the same stack AccessKit uses: no `tokio`, ADR-0038 S2). Between changes
//! the thread sleeps in the kernel, waiting on the D-Bus socket: no polling, no CPU. Our
//! connection is ours alone (AccessKit keeps its own inside its adapter): one more socket.
//!
//! The thread is not interrupted when the watch is dropped: there is no portable way to wake a
//! read blocked on a socket from another thread without closing the connection under it, and the
//! watch lives for the whole session anyway. After the drop the thread still sleeps; at the next
//! message it finds no callback and the message is ignored; it ends with the program.

use futures_lite::StreamExt;
use zbus::message::Type as MessageType;
use zbus::zvariant::{OwnedValue, Value};
use zbus::{Connection, MatchRule, Message, MessageStream};

use super::{
    AppearanceCallback, AppearanceWatch, FIRST_READ_TIMEOUT, Reporter, SystemAppearanceError,
    SystemAppearanceSource,
};
use crate::Appearance;

/// The portal's bus name.
const DESTINATION: &str = "org.freedesktop.portal.Desktop";
/// The portal's object path.
const PATH: &str = "/org/freedesktop/portal/desktop";
/// The settings interface of the portal.
const INTERFACE: &str = "org.freedesktop.portal.Settings";
/// The namespace of the appearance settings.
const NAMESPACE: &str = "org.freedesktop.appearance";
/// The key of the light/dark preference inside [`NAMESPACE`].
const KEY: &str = "color-scheme";
/// The method that reads one setting (version 2 of the interface and later).
const READ_ONE: &str = "ReadOne";
/// The deprecated method that reads one setting, wrapped in an extra variant (all versions).
const READ_DEPRECATED: &str = "Read";
/// The signal sent when a setting changes.
const SETTING_CHANGED: &str = "SettingChanged";
/// The standard D-Bus error a service returns for a method it does not have: the sign that the
/// portal is too old for `ReadOne`.
const UNKNOWN_METHOD: &str = "org.freedesktop.DBus.Error.UnknownMethod";
/// The name of the observer thread, visible in debuggers and `top -H`.
const THREAD_NAME: &str = "orchestra-appearance";
/// How many variant layers [`decode`] unwraps before giving up: one for `Read`'s extra layer, one
/// more as a margin for a portal that wraps twice. A limit, because a value is recursive and a
/// broken service must not make us recurse without end.
const MAX_VARIANT_DEPTH: u8 = 2;

/// The Linux source: the XDG desktop portal (ADR-0038 S2).
pub(crate) struct PortalSource;

impl SystemAppearanceSource for PortalSource {
    /// Starts the observer thread and waits at most [`FIRST_READ_TIMEOUT`] for the first answer.
    fn observe(&self, on_change: AppearanceCallback) -> AppearanceWatch {
        AppearanceWatch::spawn(on_change, THREAD_NAME, FIRST_READ_TIMEOUT, |reporter| {
            // `block_on` runs the future on this thread with async-io's reactor; it returns when
            // `observe_portal` does, that is when the bus goes away or the first step fails.
            zbus::block_on(observe_portal(reporter));
        })
    }
}

/// The whole conversation with the portal, in the order of the module documentation.
///
/// Every failure is reported through `reporter` (as "no answer"), never returned: there is nobody
/// to return it to on this thread.
async fn observe_portal(reporter: &Reporter) {
    // 1. The session bus. Without it there is nothing to ask and nothing to wait for.
    let connection = match Connection::session().await {
        Ok(connection) => connection,
        Err(error) => {
            reporter.fail(SystemAppearanceError::Unreachable {
                detail: error.to_string(),
            });
            return;
        }
    };

    // 2. Subscribe before reading (see the module documentation). If subscribing fails we still
    //    read once, so the initial value is right; only the changes are lost, and recorded.
    let stream = match subscribe(&connection).await {
        Ok(stream) => Some(stream),
        Err(error) => {
            reporter.fail(SystemAppearanceError::Observe {
                detail: error.to_string(),
            });
            None
        }
    };

    // 3. The current value. A failure here (no portal) is "no answer"; we keep listening anyway,
    //    because a portal started later announces itself through the same signal.
    match read(&connection).await {
        Ok(value) => reporter.answer(value),
        Err(error) => reporter.fail(error),
    }

    // 4. The changes, until the stream ends.
    let Some(mut stream) = stream else {
        return;
    };
    while let Some(message) = stream.next().await {
        match message {
            Ok(message) => forward_change(&message, reporter),
            Err(error) => {
                // A broken stream means the bus connection is gone: no more answers.
                reporter.fail(SystemAppearanceError::Observe {
                    detail: error.to_string(),
                });
                return;
            }
        }
    }
    // The stream only ends when the connection closes (the session bus went away).
    reporter.fail(SystemAppearanceError::Observe {
        detail: "the session bus closed the connection".to_owned(),
    });
}

/// Asks the bus for the portal's `SettingChanged` signals.
///
/// The match rule filters on the sender's interface, member and path, so the bus only sends us
/// these signals; filtering on our namespace and key happens in [`forward_change`], because D-Bus
/// match rules can test string arguments (`arg0`) but the gain would be small and the code longer.
///
/// # Errors
/// The library's error if the rule is invalid (it is constant, so in practice never) or the bus
/// refuses the subscription.
async fn subscribe(connection: &Connection) -> zbus::Result<MessageStream> {
    let rule = MatchRule::builder()
        .msg_type(MessageType::Signal)
        .interface(INTERFACE)?
        .member(SETTING_CHANGED)?
        .path(PATH)?
        .build();
    // `None`: the library's default queue length (64 messages), more than enough for a setting
    // that changes when the user clicks a switch.
    MessageStream::for_match_rule(rule, connection, None).await
}

/// Reads the current value with `ReadOne`, or with the deprecated `Read` on a portal that does
/// not know `ReadOne`, and decodes it.
///
/// # Errors
/// [`SystemAppearanceError::Read`] if the call fails or its reply cannot be read as a variant;
/// the errors of [`decode`] for a value of the wrong type or an unknown number.
async fn read(connection: &Connection) -> Result<Option<Appearance>, SystemAppearanceError> {
    let reply = match call(connection, READ_ONE).await {
        Ok(reply) => reply,
        // A portal older than version 2 of the interface: same arguments, old method.
        Err(zbus::Error::MethodError(name, _, _)) if name.as_str() == UNKNOWN_METHOD => {
            call(connection, READ_DEPRECATED)
                .await
                .map_err(|error| read_error(&error))?
        }
        Err(error) => return Err(read_error(&error)),
    };
    let value: OwnedValue = reply
        .body()
        .deserialize()
        .map_err(|error| read_error(&error))?;
    decode(&value)
}

/// Calls `method` (`ReadOne` or `Read`) on the portal with our namespace and key.
///
/// # Errors
/// The library's error: no portal, no such method, no such key, the bus gone.
async fn call(connection: &Connection, method: &str) -> zbus::Result<Message> {
    connection
        .call_method(
            Some(DESTINATION),
            PATH,
            Some(INTERFACE),
            method,
            &(NAMESPACE, KEY),
        )
        .await
}

/// Wraps a library error as [`SystemAppearanceError::Read`].
fn read_error(error: &zbus::Error) -> SystemAppearanceError {
    SystemAppearanceError::Read {
        detail: error.to_string(),
    }
}

/// Handles one `SettingChanged` signal: if it is about our key, decodes it and reports it.
///
/// Signals about other settings (fonts, accent colour, …) are ignored. A signal whose body is not
/// `(s, s, v)` does not follow the portal's documentation; it is ignored too, because it cannot be
/// about our key in any meaningful way and must not turn the theme dark.
fn forward_change(message: &Message, reporter: &Reporter) {
    let Ok((namespace, key, value)) = message.body().deserialize::<(String, String, OwnedValue)>()
    else {
        return;
    };
    if namespace != NAMESPACE || key != KEY {
        return;
    }
    match decode(&value) {
        Ok(appearance) => reporter.answer(appearance),
        Err(error) => reporter.fail(error),
    }
}

/// Decodes the portal's value: a `u32`, possibly wrapped in variants (the old `Read`).
///
/// # Errors
/// [`SystemAppearanceError::UnexpectedType`] for any other type, or more than
/// [`MAX_VARIANT_DEPTH`] wrappings; [`SystemAppearanceError::UnknownValue`] for a number other
/// than 0, 1 and 2.
pub(super) fn decode(value: &Value<'_>) -> Result<Option<Appearance>, SystemAppearanceError> {
    decode_at_depth(value, 0)
}

/// [`decode`] after `depth` variant layers have already been unwrapped.
///
/// # Complexity
/// O(1): at most [`MAX_VARIANT_DEPTH`] + 1 calls.
fn decode_at_depth(
    value: &Value<'_>,
    depth: u8,
) -> Result<Option<Appearance>, SystemAppearanceError> {
    match value {
        Value::U32(number) => appearance_from_portal(*number),
        Value::Value(inner) if depth < MAX_VARIANT_DEPTH => {
            decode_at_depth(inner, depth.saturating_add(1))
        }
        other => Err(SystemAppearanceError::UnexpectedType {
            signature: other.value_signature().to_string(),
        }),
    }
}

/// Maps the portal's number to an appearance (ADR-0038 S4).
///
/// # Errors
/// [`SystemAppearanceError::UnknownValue`] for a number the portal does not define.
pub(super) fn appearance_from_portal(
    value: u32,
) -> Result<Option<Appearance>, SystemAppearanceError> {
    match value {
        // 0 is "no preference", and light on the desktops that send it (GNOME's default style).
        0 | 2 => Ok(Some(Appearance::Light)),
        1 => Ok(Some(Appearance::Dark)),
        value => Err(SystemAppearanceError::UnknownValue { value }),
    }
}
