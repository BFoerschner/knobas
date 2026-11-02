//! Keys (#2429, spec #2419 stories 43 to 49): a saved list's chord, which
//! opens the launcher on the list from anywhere, and an action's chord per
//! kind, which runs that action of the Tab sheet on the selected row.
//!
//! Two tables (migration `0070`): `knobas.list_key` and `knobas.action_key`.
//! A chord is a modifier and one letter or digit, spelled `⌘⇧O`
//! ([`is_chord`]). Which chords the launcher and the shell keep for
//! themselves is the frontend's to refuse, where the keys are pressed.

use sqlx::PgPool;

use crate::SearchError;

/// A saved list's chord, with what opening it needs: the list's name and its
/// text as saved.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, sqlx::FromRow, ts_rs::TS)]
#[ts(export_to = "search.ts")]
pub struct ListKey {
    pub list_id: String,
    pub chord: String,
    pub label: String,
    pub text: String,
}

/// An action's chord on one kind: the action is the Tab sheet's row with
/// that stable id (`launcher/sheet.ts`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, sqlx::FromRow, ts_rs::TS)]
#[ts(export_to = "search.ts")]
pub struct ActionKey {
    pub kind: String,
    pub chord: String,
    pub action_id: String,
}

/// Every key bound: the lists' by chord, the actions' by kind then chord.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[ts(export_to = "search.ts")]
pub struct Keys {
    pub lists: Vec<ListKey>,
    pub actions: Vec<ActionKey>,
}

/// Whether `chord` is spelled as a chord: the modifiers in the order ⌘ ⌃ ⌥
/// ⇧ (the spec's `⌘⇧O`), at least one of ⌃ ⌥ ⌘ (⇧ alone is typing), then one capital letter or
/// digit. The tables' checks hold the same spelling.
#[must_use]
pub fn is_chord(chord: &str) -> bool {
    let mut rest = chord;
    let mut held = false;
    for modifier in ['⌘', '⌃', '⌥', '⇧'] {
        if let Some(after) = rest.strip_prefix(modifier) {
            rest = after;
            held |= modifier != '⇧';
        }
    }
    let mut chars = rest.chars();
    matches!(
        (chars.next(), chars.next()),
        (Some(key), None) if key.is_ascii_uppercase() || key.is_ascii_digit()
    ) && held
}

fn checked(chord: Option<&str>) -> Result<Option<&str>, SearchError> {
    match chord {
        Some(chord) if !is_chord(chord) => Err(SearchError::Invalid(format!(
            "{chord:?} is no chord: a modifier and one letter or digit, such as ⌘⇧O"
        ))),
        other => Ok(other),
    }
}

/// Every key bound.
///
/// # Errors
///
/// [`SearchError::Db`] if a read fails.
pub async fn read(pool: &PgPool) -> Result<Keys, SearchError> {
    let lists = sqlx::query_as::<_, ListKey>(
        "select k.list_id, k.chord, l.label, l.query as text
           from knobas.list_key k
           join knobas.smart_list l on l.id = k.list_id
          order by k.chord",
    )
    .fetch_all(pool)
    .await?;
    let actions = sqlx::query_as::<_, ActionKey>(
        "select kind, chord, action_id from knobas.action_key order by kind, chord",
    )
    .fetch_all(pool)
    .await?;
    Ok(Keys { lists, actions })
}

/// Give list `list_id` the chord, or take its chord away with `None`. A
/// chord another list held moves to this one.
///
/// # Errors
///
/// [`SearchError::Invalid`] for a chord that is none, [`SearchError::UnknownList`]
/// for a list nobody saved, [`SearchError::Db`] if the write fails.
pub async fn bind_list(
    pool: &PgPool,
    list_id: &str,
    chord: Option<&str>,
) -> Result<(), SearchError> {
    let chord = checked(chord)?;
    let mut tx = pool.begin().await?;
    sqlx::query("delete from knobas.list_key where list_id = $1 or chord = $2")
        .bind(list_id)
        .bind(chord)
        .execute(&mut *tx)
        .await?;
    if let Some(chord) = chord {
        let inserted = sqlx::query("insert into knobas.list_key (list_id, chord) values ($1, $2)")
            .bind(list_id)
            .bind(chord)
            .execute(&mut *tx)
            .await;
        match inserted {
            Ok(_) => {}
            Err(sqlx::Error::Database(error)) if error.is_foreign_key_violation() => {
                return Err(SearchError::UnknownList(list_id.to_owned()));
            }
            Err(error) => return Err(error.into()),
        }
    } else {
        let known: bool =
            sqlx::query_scalar("select exists (select 1 from knobas.smart_list where id = $1)")
                .bind(list_id)
                .fetch_one(&mut *tx)
                .await?;
        if !known {
            return Err(SearchError::UnknownList(list_id.to_owned()));
        }
    }
    tx.commit().await?;
    Ok(())
}

/// Give action `action_id` the chord on `kind`, or take its chord away with
/// `None`. A chord another action of the kind held moves to this one.
///
/// # Errors
///
/// [`SearchError::Invalid`] for a chord that is none or a blank kind or
/// action, [`SearchError::Db`] if the write fails.
pub async fn bind_action(
    pool: &PgPool,
    kind: &str,
    action_id: &str,
    chord: Option<&str>,
) -> Result<(), SearchError> {
    let chord = checked(chord)?;
    if kind.trim().is_empty() || action_id.trim().is_empty() {
        return Err(SearchError::Invalid(
            "a key names a kind and an action".into(),
        ));
    }
    let mut tx = pool.begin().await?;
    sqlx::query("delete from knobas.action_key where kind = $1 and (action_id = $2 or chord = $3)")
        .bind(kind)
        .bind(action_id)
        .bind(chord)
        .execute(&mut *tx)
        .await?;
    if let Some(chord) = chord {
        sqlx::query("insert into knobas.action_key (kind, chord, action_id) values ($1, $2, $3)")
            .bind(kind)
            .bind(chord)
            .bind(action_id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::is_chord;

    #[test]
    fn a_chord_is_a_modifier_and_one_letter_or_digit() {
        for good in ["⌘O", "⌘⇧O", "⌘⌃⌥⇧9", "⌥1", "⌃K"] {
            assert!(is_chord(good), "{good}");
        }
        for bad in ["O", "⇧O", "⌘", "⌘OO", "⌘/", "⌘o", "⇧⌘O", "⌥⌘O", "", "⌘⌘O"]
        {
            assert!(!is_chord(bad), "{bad}");
        }
    }
}
