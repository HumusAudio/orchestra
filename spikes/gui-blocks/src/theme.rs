// SPDX-License-Identifier: GPL-3.0-or-later
//! Spike F2.02b: reading the system light/dark preference, three candidates.
//!
//! `gui-blocks theme` prints the preference once, then prints every change until Ctrl+C. Each
//! candidate is a cargo feature, so the binary can be built with exactly one of them and its cost
//! read off the size and the crate count:
//! - `theme-zbus`: our own code on `zbus` 5.19 with AccessKit's features (`async-io`, no
//!   `blocking-api`): `ReadOne` on `org.freedesktop.portal.Settings`, then the `SettingChanged`
//!   signal through a match rule. No proc macros of ours.
//! - `theme-darklight`: `dark-light` 3.0 (`detect` + `subscribe`).
//! - `theme-mundy`: `mundy` 0.2 (`Preferences::stream`), colour scheme only.

/// Entry point of the `theme` mode: dispatches to the candidate compiled in.
pub fn run() {
    #[cfg(feature = "theme-zbus")]
    zbus_portal::run();
    #[cfg(feature = "theme-darklight")]
    darklight::run();
    #[cfg(feature = "theme-mundy")]
    mundy_stream::run();
    #[cfg(not(any(
        feature = "theme-zbus",
        feature = "theme-darklight",
        feature = "theme-mundy"
    )))]
    eprintln!("built without a theme-* feature");
}

#[cfg(feature = "theme-zbus")]
mod zbus_portal {
    use futures_lite::StreamExt;
    use zbus::message::Type;
    use zbus::zvariant::OwnedValue;
    use zbus::{Connection, MatchRule, MessageStream};

    const DEST: &str = "org.freedesktop.portal.Desktop";
    const PATH: &str = "/org/freedesktop/portal/desktop";
    const IFACE: &str = "org.freedesktop.portal.Settings";
    const NS: &str = "org.freedesktop.appearance";
    const KEY: &str = "color-scheme";

    /// 0 = no preference, 1 = dark, 2 = light (portal documentation).
    fn name(v: u32) -> &'static str {
        match v {
            1 => "dark",
            2 => "light",
            _ => "no preference",
        }
    }

    pub fn run() {
        // `zbus::block_on` is async-io's executor: the same one AccessKit uses on its thread.
        let result: zbus::Result<()> = zbus::block_on(async {
            let conn = Connection::session().await?;
            let reply = conn
                .call_method(Some(DEST), PATH, Some(IFACE), "ReadOne", &(NS, KEY))
                .await?;
            let value: OwnedValue = reply.body().deserialize()?;
            let v = u32::try_from(value).unwrap_or(0);
            println!("now: {}", name(v));
            let rule = MatchRule::builder()
                .msg_type(Type::Signal)
                .interface(IFACE)?
                .member("SettingChanged")?
                .path(PATH)?
                .build();
            let mut stream = MessageStream::for_match_rule(rule, &conn, None).await?;
            while let Some(msg) = stream.next().await {
                let msg = msg?;
                let (ns, key, value): (String, String, OwnedValue) = msg.body().deserialize()?;
                if ns == NS && key == KEY {
                    println!("changed: {}", name(u32::try_from(value).unwrap_or(0)));
                }
            }
            Ok(())
        });
        if let Err(e) = result {
            eprintln!("portal error: {e}");
        }
    }
}

#[cfg(feature = "theme-darklight")]
mod darklight {
    pub fn run() {
        println!("now: {:?}", dark_light::detect());
        match dark_light::subscribe() {
            Ok(w) => {
                for mode in w.iter() {
                    println!("changed: {mode:?}");
                }
            }
            Err(e) => eprintln!("subscribe error: {e}"),
        }
    }
}

#[cfg(feature = "theme-mundy")]
mod mundy_stream {
    use futures_lite::StreamExt;
    use mundy::{Interest, Preferences};

    pub fn run() {
        async_io::block_on(async {
            let mut stream = Preferences::stream(Interest::ColorScheme);
            while let Some(p) = stream.next().await {
                println!("preference: {:?}", p.color_scheme);
            }
        });
    }
}
