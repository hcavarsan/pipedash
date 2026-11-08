use super::*;

#[test]
fn slices_pages_and_reports_has_more() {
    let page = paginate(
        (0..5).collect(),
        PaginationParams {
            page: 2,
            page_size: 2,
        },
    )
    .unwrap();
    assert_eq!(page.items, [2, 3]);
    assert!(page.has_more);
    assert_eq!(page.total_pages, 3);

    let last = paginate(
        (0..5).collect(),
        PaginationParams {
            page: 3,
            page_size: 2,
        },
    )
    .unwrap();
    assert_eq!(last.items, [4]);
    assert!(!last.has_more);
}

#[test]
fn rejects_invalid_params() {
    assert!(paginate(
        vec![1],
        PaginationParams {
            page: 0,
            page_size: 1
        }
    )
    .is_err());
    assert!(paginate(
        vec![1],
        PaginationParams {
            page: usize::MAX,
            page_size: 200
        }
    )
    .is_err());
}
