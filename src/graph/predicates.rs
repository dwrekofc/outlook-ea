use std::borrow::Cow;

// Drop underscore fallback next release, after vault migrates existing edges.
pub(super) fn canonical_predicate(predicate: &str) -> Cow<'_, str> {
    if predicate.contains('_') {
        Cow::Owned(predicate.replace('_', "-"))
    } else {
        Cow::Borrowed(predicate)
    }
}

#[cfg(test)]
mod tests {
    use super::canonical_predicate;
    use crate::db::test_store;
    use crate::graph::*;
    use libsql::params;

    const PREDICATES: &[&str] = &[
        "belongs-to",
        "member-of",
        "reports-to",
        "works-on",
        "contact-for",
        "collaborates-with",
        "applies-action",
        "matches-sender",
        "matches-subject",
        "applies-action-label",
        "expert-in",
        "protects",
    ];

    #[test]
    fn canonicalizes_legacy_spelling_and_preserves_canonical_predicates() {
        for predicate in PREDICATES {
            assert_eq!(canonical_predicate(predicate), *predicate);
            assert_eq!(
                canonical_predicate(&predicate.replace('-', "_")),
                *predicate
            );
        }
    }

    #[test]
    fn write_guard_rejects_underscores_and_uppercase_without_side_effects() {
        let store = test_store().unwrap();
        let a = add_node(&store, "person", "A", None, None, None, false).unwrap();
        let b = add_node(&store, "team", "B", None, None, None, false).unwrap();
        for invalid in [
            "member-of".replace('-', "_"),
            "Member-of".into(),
            "Équipe".into(),
        ] {
            let error = add_edge(&store, a, b, &invalid, None, None).unwrap_err();
            assert!(matches!(error, GraphError::InvalidPredicate(_)));
            assert!(error.to_string().contains("lowercase hyphenated"));
        }
        assert!(get_edges(&store, a, None).unwrap().is_empty());
        assert_eq!(
            store
                .one("SELECT count(*) FROM graph_history", (), |r| Ok(
                    r.get::<i64>(0)?
                ))
                .unwrap(),
            Some(2)
        );
        for predicate in PREDICATES {
            add_edge(&store, a, b, predicate, None, None).unwrap();
        }
    }

    #[test]
    fn legacy_edges_support_filters_traversal_and_dump() {
        let store = test_store().unwrap();
        let a = add_node(&store, "person", "A", None, None, None, false).unwrap();
        let b = add_node(&store, "team", "B", None, None, None, false).unwrap();
        for predicate in PREDICATES {
            let legacy = predicate.replace('-', "_");
            let id = add_edge(&store, a, b, predicate, None, None).unwrap();
            store
                .execute(
                    "UPDATE graph_edges SET predicate=?1 WHERE id=?2",
                    params![legacy.clone(), id],
                )
                .unwrap();
            for filter in [*predicate, legacy.as_str()] {
                let edges = get_edges(&store, a, Some(filter)).unwrap();
                assert_eq!(edges.len(), 1);
                assert_eq!(edges[0].edge.predicate, *predicate);
                let traversal = traverse(&store, a, Some(filter), 1).unwrap();
                assert_eq!(traversal.len(), 2);
                assert_eq!(traversal[1].path, vec![predicate.to_string()]);
            }
        }
        assert!(dump_context(&store).unwrap().contains("--[member-of]-->"));
    }

    #[test]
    fn tasks_and_rules_read_both_spellings() {
        let store = test_store().unwrap();
        let project = add_project(&store, "Project", None).unwrap();
        let task = add_task(&store, "Task", None, None, Some(project)).unwrap();
        add_vip(&store, "Ada", "ada@test", None, None).unwrap();
        let subject = add_rule(&store, "Subject", "subject", "news", "trash", "").unwrap();
        let topic = get_edges(&store, subject, None).unwrap()[0].edge.target_id;
        add_edge(
            &store,
            subject,
            topic,
            "applies-action-trash",
            Some("trash"),
            None,
        )
        .unwrap();
        for legacy in [false, true] {
            if legacy {
                store
                    .execute_batch("UPDATE graph_edges SET predicate=replace(predicate,'-','_')")
                    .unwrap();
            }
            assert_eq!(list_tasks(&store, Some(project), None).unwrap()[0].id, task);
            let rules = get_all_rules(&store).unwrap();
            let vip = rules
                .iter()
                .find(|r| r.rule_node.name == "VIP: Ada")
                .unwrap();
            assert_eq!(vip.match_type.as_deref(), Some("sender"));
            assert_eq!(vip.match_value.as_deref(), Some("ada@test"));
            assert_eq!(vip.action_type.as_deref(), Some("label:1"));
            let subject = rules
                .iter()
                .find(|r| r.rule_node.name == "Subject")
                .unwrap();
            assert_eq!(subject.match_type.as_deref(), Some("subject"));
            assert_eq!(subject.match_value.as_deref(), Some("news"));
            assert_eq!(subject.action_type.as_deref(), Some("trash"));
        }
    }
}
