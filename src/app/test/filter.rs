use super::*;
use crate::app::filter::CriteriaRelation;

#[tokio::test]
async fn test_filter() {
    let mut app = create_default_app();
    app.load_entries().await.unwrap();

    app.current_entry_id = Some(0);

    let mut filter = Filter::default();
    filter
        .criteria
        .push(FilterCriterion::Title(String::from("Title 2")));
    app.apply_filter(Some(filter));

    assert_eq!(app.get_active_entries().count(), 1);
    assert!(app.get_current_entry().is_none());
    let entry = app.get_active_entries().next().unwrap();
    assert_eq!(entry.id, 1);
    assert_eq!(entry.title, String::from("Title 2"));
    assert!(app.get_entry(0).is_none());

    app.apply_filter(None);
    assert_eq!(app.get_active_entries().count(), 2);
}

#[tokio::test]
async fn test_title_smart_case() {
    let mut app = create_default_app();
    app.load_entries().await.unwrap();

    app.current_entry_id = Some(0);
    let mut filter = Filter::default();
    filter
        .criteria
        .push(FilterCriterion::Title(String::from("title 2")));
    app.apply_filter(Some(filter));

    assert_eq!(app.get_active_entries().count(), 1);
    assert!(app.get_current_entry().is_none());
    let entry = app.get_active_entries().next().unwrap();
    assert_eq!(entry.id, 1);
    assert_eq!(entry.title, String::from("Title 2"));
    assert!(app.get_entry(0).is_none());

    app.apply_filter(None);
    assert_eq!(app.get_active_entries().count(), 2);
}

#[tokio::test]
async fn test_content_smart_case() {
    let mut app = create_default_app();
    app.load_entries().await.unwrap();

    app.current_entry_id = Some(0);
    let mut filter = Filter::default();
    filter
        .criteria
        .push(FilterCriterion::Content(String::from("content 2")));
    app.apply_filter(Some(filter));

    assert_eq!(app.get_active_entries().count(), 1);
    assert!(app.get_current_entry().is_none());
    let entry = app.get_active_entries().next().unwrap();
    assert_eq!(entry.id, 1);
    assert_eq!(entry.content, String::from("Content 2"));
    assert!(app.get_entry(0).is_none());

    app.apply_filter(None);
    assert_eq!(app.get_active_entries().count(), 2);
}

#[tokio::test]
async fn test_filter_priority() {
    let mut app = create_default_app();
    app.load_entries().await.unwrap();

    app.current_entry_id = Some(0);

    let mut filter = Filter::default();
    filter.criteria.push(FilterCriterion::Priority(1));
    app.apply_filter(Some(filter));

    assert_eq!(app.get_active_entries().count(), 1);
    assert!(app.get_current_entry().is_none());
    let entry = app.get_active_entries().next().unwrap();
    assert_eq!(entry.id, 1);
    assert_eq!(entry.priority, Some(1));
    assert!(app.get_entry(0).is_none());

    app.apply_filter(None);
    assert_eq!(app.get_active_entries().count(), 2);
}

#[tokio::test]
async fn test_filter_relations() {
    let mut app = create_default_app();
    app.load_entries().await.unwrap();
    let criteria = vec![
        FilterCriterion::Content("1".into()),
        FilterCriterion::Content("2".into()),
    ];

    let mut filter = Filter {
        criteria,
        relation: CriteriaRelation::Or,
    };

    app.apply_filter(Some(filter.clone()));

    assert_eq!(app.get_active_entries().count(), 2);

    filter.relation = CriteriaRelation::And;
    app.apply_filter(Some(filter));

    assert_eq!(app.get_active_entries().count(), 0);
}

#[tokio::test]
async fn cycle_tag_no_tags() {
    let mut app = App::new(MockDataProvider::default(), Settings::default());
    app.load_entries().await.unwrap();

    // Check empty app doesn't panic
    app.cycle_tags_in_filter();

    app.add_entry("Title_1".into(), Utc::now(), Vec::new(), Some(1))
        .await
        .unwrap();
    app.add_entry("Title_2".into(), Utc::now(), Vec::new(), Some(2))
        .await
        .unwrap();

    // No panic on cycle with not tags
    app.cycle_tags_in_filter();
}

#[tokio::test]
async fn cycle_tag_no_existing_filter() {
    // default project has two tags
    let mut app = create_default_app();
    app.load_entries().await.unwrap();

    for _ in 0..3 {
        app.cycle_tags_in_filter();

        // Filter exits and have one criteria
        assert_eq!(app.filter.as_ref().unwrap().criteria.len(), 1);

        // Check the criteria
        let criteria = app
            .filter
            .as_ref()
            .and_then(|f| f.criteria.first())
            .unwrap();
        assert!(
            matches!(criteria, FilterCriterion::Tag(_)),
            "Expected Tag criteria. found {criteria:?}"
        );
    }
}

#[tokio::test]
async fn cycle_tag_exact() {
    // default project has two tags "Tag 1" "Tag 2"
    let mut app = create_default_app();
    app.load_entries().await.unwrap();

    app.cycle_tags_in_filter();

    // First iteration must be first tag
    match app
        .filter
        .as_ref()
        .and_then(|f| f.criteria.first())
        .unwrap()
    {
        FilterCriterion::Tag(s) => assert_eq!(s, "Tag 1"),
        invalid => panic!("Invalid criteria: {invalid:?}"),
    }

    app.cycle_tags_in_filter();

    // Second iteration must be second tag
    match app
        .filter
        .as_ref()
        .and_then(|f| f.criteria.first())
        .unwrap()
    {
        FilterCriterion::Tag(s) => assert_eq!(s, "Tag 2"),
        invalid => panic!("Invalid criteria: {invalid:?}"),
    }

    // 3rd iteration is for untagged entries
    app.cycle_tags_in_filter();

    // 4th iteration must go back to first tag
    app.cycle_tags_in_filter();
    match app
        .filter
        .as_ref()
        .and_then(|f| f.criteria.first())
        .unwrap()
    {
        FilterCriterion::Tag(s) => assert_eq!(s, "Tag 1"),
        invalid => panic!("Invalid criteria: {invalid:?}"),
    }
}

#[tokio::test]
async fn cycle_tag_existing_filter() {
    // default project has two tags "Tag 1" "Tag 2"
    let mut app = create_default_app();
    app.load_entries().await.unwrap();
    app.add_entry(
        "Title_3".into(),
        Utc::now(),
        vec!["New".into(), "Other".into()],
        Some(55),
    )
    .await
    .unwrap();

    let mut filter = Filter::default();
    filter.criteria.push(FilterCriterion::Title("Title".into()));
    app.apply_filter(Some(filter));

    app.cycle_tags_in_filter();

    // Filter exits and have two criteria
    assert_eq!(app.filter.as_ref().unwrap().criteria.len(), 2);

    // Criteria must be one tag
    assert_eq!(
        app.filter
            .as_ref()
            .unwrap()
            .criteria
            .iter()
            .filter(|c| matches!(c, FilterCriterion::Tag(_)))
            .count(),
        1
    );
    // Criteria must be one title
    assert_eq!(
        app.filter
            .as_ref()
            .unwrap()
            .criteria
            .iter()
            .filter(|c| matches!(c, FilterCriterion::Title(_)))
            .count(),
        1
    );
}
