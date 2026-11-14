use super::*;
use crate::app_update::{CatalogResult, catalog::fetch_catalog_with};
use std::collections::BTreeMap;

struct CatalogFixture {
    release: Value,
    bodies: BTreeMap<String, Vec<u8>>,
    requests: RefCell<Vec<String>>,
    next_id: u64,
}

impl CatalogFixture {
    fn new() -> Self {
        let mut release = Fixture::new(manifest_fixture()).release;
        release["assets"].as_array_mut().unwrap().remove(0);
        Self {
            release,
            bodies: BTreeMap::new(),
            requests: RefCell::new(Vec::new()),
            next_id: 200,
        }
    }

    fn add_manifest(&mut self, name: &str, revision: u64) {
        let mut manifest = manifest_fixture();
        manifest["build_revision"] = json!(revision);
        self.add_body(name, serde_json::to_vec(&manifest).unwrap());
    }

    fn add_body(&mut self, name: &str, body: Vec<u8>) {
        self.release["assets"]
            .as_array_mut()
            .unwrap()
            .push(release_asset(
                self.next_id,
                name,
                body.len() as u64,
                Some(&sha256_hex(&body)),
            ));
        self.next_id += 1;
        self.bodies.insert(name.into(), body);
    }

    fn read(
        &self,
        url: &str,
        max: usize,
        timeout: Duration,
        token: Arc<AtomicBool>,
    ) -> Result<Vec<u8>> {
        assert_eq!(timeout, Duration::from_secs(30));
        assert!(!token.load(std::sync::atomic::Ordering::Relaxed));
        self.requests.borrow_mut().push(url.into());
        if url == RELEASE_API {
            return Ok(serde_json::to_vec(&self.release).unwrap());
        }
        let asset = self.release["assets"]
            .as_array()
            .unwrap()
            .iter()
            .find(|asset| asset["browser_download_url"] == url)
            .expect("Only exact official asset URLs may be read");
        assert_eq!(max as u64, asset["size"].as_u64().unwrap());
        self.bodies
            .get(asset["name"].as_str().unwrap())
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("HTTP 404"))
    }

    fn fetch(&self) -> Result<CatalogResult> {
        fetch_catalog_with(cancel(), &|_| {}, &|url, max, timeout, token| {
            self.read(url, max, timeout, token)
        })
    }

    fn assert_only_selected_manifest_was_read(&self, name: &str) {
        let requests = self.requests.borrow();
        assert_eq!(
            requests.len(),
            2,
            "Must not retry any older manifest: {requests:?}"
        );
        assert_eq!(requests[0], RELEASE_API);
        assert!(requests[1].ends_with(&format!("/{name}")), "{requests:?}");
    }
}

#[test]
fn highest_numeric_revision_wins_independently_of_asset_order_and_legacy_alias() {
    let mut fixture = CatalogFixture::new();
    fixture.add_manifest("UPDATE-MANIFEST-9.json", 9);
    fixture.add_manifest("UPDATE-MANIFEST-10.json", 10);
    fixture.add_manifest(MANIFEST_ASSET, 999);
    let original = fixture.release["assets"].as_array().unwrap().clone();
    for reverse in [false, true] {
        for rotation in 0..original.len() {
            let mut order = original.clone();
            order.rotate_left(rotation);
            if reverse {
                order.reverse();
            }
            fixture.release["assets"] = json!(order);
            fixture.requests.borrow_mut().clear();
            let CatalogResult::Available(catalog) = fixture.fetch().unwrap() else {
                panic!("Expected catalog");
            };
            assert_eq!(catalog.manifest().build_revision, 10);
            fixture.assert_only_selected_manifest_was_read("UPDATE-MANIFEST-10.json");
        }
    }
}

#[test]
fn selected_filename_revision_must_equal_the_parsed_payload_revision() {
    let mut fixture = CatalogFixture::new();
    fixture.add_manifest(MANIFEST_ASSET, 1);
    fixture.add_manifest("UPDATE-MANIFEST-2.json", 2);
    fixture.add_manifest("UPDATE-MANIFEST-3.json", 2);
    let error = fixture.fetch().unwrap_err();
    assert!(
        error
            .to_string()
            .contains("does not match its asset filename")
    );
    fixture.assert_only_selected_manifest_was_read("UPDATE-MANIFEST-3.json");
}

#[test]
fn fixed_legacy_catalog_is_used_only_without_a_versioned_manifest() {
    let mut fixture = CatalogFixture::new();
    fixture.add_manifest(MANIFEST_ASSET, 2026111402);
    fixture.add_body(
        "UPDATE-MANIFEST-999.json.bak",
        b"This is not a catalog".to_vec(),
    );
    let CatalogResult::Available(catalog) = fixture.fetch().unwrap() else {
        panic!("Expected legacy catalog");
    };
    assert_eq!(catalog.manifest().build_revision, 2026111402);
    fixture.assert_only_selected_manifest_was_read(MANIFEST_ASSET);
}

#[test]
fn malformed_versioned_names_fail_instead_of_activating_the_legacy_catalog() {
    for digits in [
        "",
        "0",
        "01",
        "00010",
        "+10",
        "-1",
        "1.0",
        "1e3",
        "latest",
        "18446744073709551616",
    ] {
        let mut fixture = CatalogFixture::new();
        fixture.add_manifest(MANIFEST_ASSET, 1);
        fixture.add_manifest(&format!("UPDATE-MANIFEST-{digits}.json"), 10);
        assert!(fixture.fetch().is_err(), "{digits}");
        assert_eq!(
            fixture.requests.borrow().len(),
            1,
            "Invalid revision must fail before downloading a manifest: {digits}"
        );
    }
}

#[test]
fn duplicate_canonical_revisions_are_ambiguous_even_with_different_asset_ids() {
    let mut fixture = CatalogFixture::new();
    fixture.add_manifest("UPDATE-MANIFEST-3.json", 3);
    fixture.add_manifest("UPDATE-MANIFEST-3.json", 3);
    let error = fixture.fetch().unwrap_err();
    assert!(
        error
            .to_string()
            .contains("Duplicate or invalid release asset identity")
    );
    assert_eq!(fixture.requests.borrow().len(), 1);
}

#[test]
fn invalid_highest_catalog_never_retries_older_or_legacy_manifests() {
    for failure in ["json", "digest", "bounds"] {
        let mut fixture = CatalogFixture::new();
        fixture.add_manifest(MANIFEST_ASSET, 1);
        fixture.add_manifest("UPDATE-MANIFEST-2.json", 2);
        if failure == "json" {
            fixture.add_body("UPDATE-MANIFEST-3.json", b"{invalid-json}".to_vec());
        } else {
            fixture.add_manifest("UPDATE-MANIFEST-3.json", 3);
            let highest = fixture.release["assets"]
                .as_array_mut()
                .unwrap()
                .last_mut()
                .unwrap();
            if failure == "digest" {
                highest["digest"] = json!(format!("sha256:{}", "b".repeat(64)));
            } else {
                highest["size"] = json!(65537);
            }
        }
        assert!(fixture.fetch().is_err(), "{failure}");
        if failure == "bounds" {
            assert_eq!(fixture.requests.borrow().len(), 1);
        } else {
            fixture.assert_only_selected_manifest_was_read("UPDATE-MANIFEST-3.json");
        }
    }
}

#[test]
fn unavailable_highest_catalog_returns_manual_without_reading_an_older_manifest() {
    let mut fixture = CatalogFixture::new();
    fixture.add_manifest(MANIFEST_ASSET, 1);
    fixture.add_manifest("UPDATE-MANIFEST-2.json", 2);
    fixture.add_manifest("UPDATE-MANIFEST-3.json", 3);
    fixture.bodies.remove("UPDATE-MANIFEST-3.json");
    assert!(
        matches!(fixture.fetch().unwrap(), CatalogResult::Manual { reason, .. } if reason.contains("unavailable"))
    );
    fixture.assert_only_selected_manifest_was_read("UPDATE-MANIFEST-3.json");
}

#[test]
fn filename_revision_uses_the_full_u64_range_without_precision_loss() {
    let mut fixture = CatalogFixture::new();
    fixture.add_manifest("UPDATE-MANIFEST-18446744073709551614.json", u64::MAX - 1);
    fixture.add_manifest("UPDATE-MANIFEST-18446744073709551615.json", u64::MAX);
    let CatalogResult::Available(catalog) = fixture.fetch().unwrap() else {
        panic!("Expected catalog");
    };
    assert_eq!(catalog.manifest().build_revision, u64::MAX);
    fixture.assert_only_selected_manifest_was_read("UPDATE-MANIFEST-18446744073709551615.json");
}
