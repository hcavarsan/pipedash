pub(crate) mod backend;
pub(crate) mod tokens;

use std::collections::{
    HashMap,
    HashSet,
};
use std::future::Future;
use std::sync::{
    Arc,
    Mutex,
    PoisonError,
};

use crate::domain::ProviderConfig;

tokio::task_local! {
    static REVEALED: ();
}

#[derive(Default)]
struct Hidden {
    rows: HashMap<i64, ProviderConfig>,
    tokens: HashMap<i64, Option<String>>,
    created_names: HashSet<String>,
    created_ids: HashSet<i64>,
}

impl Hidden {
    fn row(&self, row: ProviderConfig) -> Option<ProviderConfig> {
        let id = row.id;
        if let Some(prior) = id.and_then(|id| self.rows.get(&id)) {
            return Some(prior.clone());
        }
        let created = id.is_some_and(|id| self.created_ids.contains(&id))
            || self.created_names.contains(&row.name);

        (!created).then_some(row)
    }
}

#[derive(Default)]
pub(crate) struct ReloadShadow {
    hidden: Mutex<Option<Hidden>>,
}

pub(crate) struct ShadowGuard {
    shadow: Option<Arc<ReloadShadow>>,
}

impl ReloadShadow {
    pub(crate) async fn reveal<F: Future>(work: F) -> F::Output {
        REVEALED.scope((), work).await
    }

    pub(crate) fn hide(
        self: &Arc<Self>, priors: Vec<(ProviderConfig, Option<String>)>, created: HashSet<String>,
    ) -> ShadowGuard {
        let mut hidden = Hidden {
            created_names: created,
            ..Hidden::default()
        };
        for (row, token) in priors {
            if let Some(id) = row.id {
                hidden.tokens.insert(id, token);
                hidden.rows.insert(id, row);
            }
        }
        *self.hidden.lock().unwrap_or_else(PoisonError::into_inner) = Some(hidden);

        ShadowGuard {
            shadow: Some(Arc::clone(self)),
        }
    }

    fn view<R>(&self, read: impl FnOnce(&Hidden) -> R) -> Option<R> {
        if REVEALED.try_with(|()| ()).is_ok() {
            return None;
        }

        self.hidden
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .map(read)
    }

    pub(crate) fn rows(&self, rows: Vec<ProviderConfig>) -> Vec<ProviderConfig> {
        let Some(visible) = self.view(|hidden| {
            rows.iter()
                .filter_map(|row| hidden.row(row.clone()))
                .collect()
        }) else {
            return rows;
        };

        visible
    }

    pub(crate) fn row(&self, row: Option<ProviderConfig>) -> Option<ProviderConfig> {
        let found = row?;

        self.view(|hidden| hidden.row(found.clone()))
            .unwrap_or(Some(found))
    }

    pub(crate) fn token(&self, id: i64) -> Option<Option<String>> {
        self.view(|hidden| {
            hidden
                .tokens
                .get(&id)
                .cloned()
                .or_else(|| hidden.created_ids.contains(&id).then_some(None))
        })
        .flatten()
    }

    pub(crate) fn tokens(&self, mut tokens: HashMap<i64, String>) -> HashMap<i64, String> {
        self.view(|hidden| {
            hidden.created_ids.iter().for_each(|id| {
                tokens.remove(id);
            });
            for (id, prior) in &hidden.tokens {
                match prior {
                    Some(token) => tokens.insert(*id, token.clone()),
                    None => tokens.remove(id),
                };
            }
        });

        tokens
    }
}

impl ShadowGuard {
    pub(crate) fn none() -> Self {
        Self { shadow: None }
    }

    pub(crate) fn created(&self, id: i64) {
        let Some(shadow) = &self.shadow else {
            return;
        };
        if let Some(hidden) = shadow
            .hidden
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_mut()
        {
            hidden.created_ids.insert(id);
        }
    }
}

impl Drop for ShadowGuard {
    fn drop(&mut self) {
        if let Some(shadow) = &self.shadow {
            *shadow.hidden.lock().unwrap_or_else(PoisonError::into_inner) = None;
        }
    }
}
