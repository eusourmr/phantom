//! Phantom 2D-5 browser resource-budget security tests.

use super::*;

fn loaded_tab(html: &str) -> Result<BrowserTab, Box<dyn std::error::Error>> {
    let mut tab = BrowserTab::new();
    tab.engine.load_html(html)?;
    tab.mark_navigation_ready();
    Ok(tab)
}

#[test]
fn site_icon_candidates_are_capped_and_keep_fallback_slot() -> Result<(), Box<dyn std::error::Error>>
{
    let mut html = String::new();
    for index in 0..32 {
        html.push_str(&format!(r#"<link rel="icon" href="/icon-{index}.png">"#));
    }

    let tab = loaded_tab(&html)?;
    let base = HttpUrl::parse("https://example.com/page")?;
    let candidates = collect_site_icon_candidates(&tab, &base);

    assert!(candidates.len() <= MAX_SITE_ICON_CANDIDATES);
    assert!(
        candidates
            .iter()
            .any(|candidate| candidate.as_str() == "https://example.com/favicon.ico")
    );

    Ok(())
}

#[test]
fn preload_discovery_is_capped() -> Result<(), Box<dyn std::error::Error>> {
    let mut html = String::new();
    for index in 0..64 {
        html.push_str(&format!(
            r#"<link rel="preload" as="image" href="/preload-{index}.png">"#
        ));
    }

    let tab = loaded_tab(&html)?;
    let base = HttpUrl::parse("https://example.com/page")?;
    let preloads = collect_preload_requests(&tab, &base, 1.0);

    assert!(preloads.len() <= MAX_IMAGE_PRELOADS_PER_DOCUMENT);
    Ok(())
}

#[test]
fn preload_and_image_for_same_url_are_merged() -> Result<(), Box<dyn std::error::Error>> {
    let mut tab = loaded_tab(
        r#"<link rel="preload" as="image" href="/hero.png" fetchpriority="high">
           <img src="/hero.png" fetchpriority="high">"#,
    )?;
    let base = HttpUrl::parse("https://example.com/page")?;

    let requests = collect_document_image_requests(&mut tab, &base, 1.0);

    assert_eq!(requests.len(), 1);
    assert!(!requests[0].preload_only);
    assert_eq!(requests[0].resources.len(), 1);
    Ok(())
}

#[test]
fn aggregate_resource_request_count_is_capped() -> Result<(), Box<dyn std::error::Error>> {
    let mut html = String::new();

    for index in 0..64 {
        html.push_str(&format!(r#"<img src="/image-{index}.png">"#));
    }

    for index in 0..32 {
        html.push_str(&format!(
            r#"<link rel="preload" as="image" href="/preload-{index}.png">"#
        ));
    }

    let mut tab = loaded_tab(&html)?;
    let base = HttpUrl::parse("https://example.com/page")?;
    let requests = collect_document_image_requests(&mut tab, &base, 1.0);

    assert!(requests.len() <= MAX_IMAGE_RESOURCE_REQUESTS_PER_DOCUMENT);
    Ok(())
}

#[test]
fn subresource_budget_reservation_is_fail_closed() {
    let mut budget = SubresourceBudget {
        remaining_fetches: 2,
        remaining_body_bytes: 10,
    };

    assert_eq!(budget.reserve(8), Some(8));
    assert_eq!(budget.reserve(8), Some(2));
    assert_eq!(budget.reserve(1), None);
}

#[test]
fn successful_small_response_refunds_unused_reserved_bytes() {
    let mut budget = SubresourceBudget {
        remaining_fetches: 1,
        remaining_body_bytes: 100,
    };

    let reserved = budget.reserve(80);
    assert_eq!(reserved, Some(80));

    if let Some(reserved) = reserved {
        budget.refund_unused(reserved, 20);
    }

    assert_eq!(budget.remaining_body_bytes, 80);
    assert_eq!(budget.remaining_fetches, 0);
}

#[test]
fn cancelling_tab_resource_work_cancels_site_icon_worker() {
    let (_sender, receiver) = mpsc::channel::<Result<LoadedSiteIcon, String>>();
    let cancelled = Arc::new(AtomicBool::new(false));

    let mut tab = BrowserTab::new();
    tab.pending_site_icon = Some(PendingSiteIcon {
        receiver,
        generation: 1,
        cancelled: Arc::clone(&cancelled),
    });

    tab.cancel_image_work();

    assert!(cancelled.load(Ordering::Acquire));
    assert!(tab.pending_site_icon.is_none());
}
