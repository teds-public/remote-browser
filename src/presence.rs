use crate::types::{Presence, StreamQuality, TabId, ViewerPrefs};
use parking_lot::RwLock;
use rand::Rng;
use std::collections::HashMap;

pub struct PresenceManager {
    users: RwLock<HashMap<String, Presence>>,
}

impl PresenceManager {
    pub fn new() -> Self {
        Self {
            users: RwLock::new(HashMap::new()),
        }
    }

    pub fn join(&self, socket_id: String, name: String, prefs: ViewerPrefs) -> Presence {
        let name = sanitize_name(name);
        let presence = Presence::new(socket_id.clone(), name, prefs);

        self.users.write().insert(socket_id, presence.clone());
        presence
    }

    pub fn leave(&self, socket_id: &str) -> Option<Presence> {
        self.users.write().remove(socket_id)
    }

    pub fn rename(&self, socket_id: &str, name: String) -> Option<()> {
        let mut users = self.users.write();
        let presence = users.get_mut(socket_id)?;
        presence.name = sanitize_name(name);
        Some(())
    }

    pub fn set_watching_tab(&self, socket_id: &str, tab_id: Option<TabId>) -> Option<()> {
        let mut users = self.users.write();
        let presence = users.get_mut(socket_id)?;
        presence.tab_id = tab_id;
        Some(())
    }

    pub fn update_prefs(&self, socket_id: &str, prefs: ViewerPrefs) -> Option<()> {
        let mut users = self.users.write();
        let presence = users.get_mut(socket_id)?;
        presence.prefs = prefs;
        Some(())
    }

    pub fn get_watching_tab(&self, socket_id: &str) -> Option<TabId> {
        self.users.read().get(socket_id)?.tab_id
    }

    pub fn name_of(&self, socket_id: &str) -> Option<String> {
        self.users.read().get(socket_id).map(|p| p.name.clone())
    }

    pub fn get_viewers_for_tab(&self, tab_id: TabId) -> Vec<(String, ViewerPrefs)> {
        self.users
            .read()
            .values()
            .filter(|p| p.tab_id == Some(tab_id))
            .map(|p| (p.socket_id.clone(), p.prefs.clone()))
            .collect()
    }

    pub fn viewers_by_tab(&self) -> HashMap<TabId, Vec<String>> {
        let mut result: HashMap<TabId, Vec<String>> = HashMap::new();

        for presence in self.users.read().values() {
            if let Some(tab_id) = presence.tab_id {
                result
                    .entry(tab_id)
                    .or_default()
                    .push(presence.name.clone());
            }
        }

        result
    }

    pub fn aggregate_stream_prefs_for_tab(&self, tab_id: TabId) -> Option<ViewerPrefs> {
        let viewers = self.get_viewers_for_tab(tab_id);

        if viewers.is_empty() {
            return None;
        }

        let all_paused = viewers.iter().all(|(_, prefs)| prefs.paused);
        if all_paused {
            return Some(ViewerPrefs {
                quality: StreamQuality::Low,
                paused: true,
            });
        }

        let highest_quality = viewers
            .iter()
            .filter(|(_, prefs)| !prefs.paused)
            .map(|(_, prefs)| prefs.quality)
            .max_by_key(|q| match q {
                StreamQuality::Low => 0,
                StreamQuality::Med => 1,
                StreamQuality::High => 2,
            })
            .unwrap_or(StreamQuality::Med);

        Some(ViewerPrefs {
            quality: highest_quality,
            paused: false,
        })
    }
}

fn sanitize_name(name: String) -> String {
    let name = name
        .trim()
        .chars()
        .filter(|c| !c.is_control())
        .take(30)
        .collect::<String>();

    if name.is_empty() {
        let num = rand::thread_rng().gen_range(1000..10000);
        format!("Guest {}", num)
    } else {
        name
    }
}
