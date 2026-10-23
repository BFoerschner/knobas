//! A live server that is slow to answer a connection (#1815).
//!
//! The five-second bound on a first connection is for `KNOBAS_DB_URL`'s first
//! contact, where nothing has answered yet and a dead URL must not hold the
//! boot screen. A pool opened on a server that has already answered is waited
//! for under the pool's own acquire timeout instead: on a loaded machine a
//! connection to a live local server took longer than five seconds, and the
//! bound reported "no answer" from a server that was up.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use knobas_db::{DbConfig, EmbeddedDb};

/// Longer than the five seconds a first contact gets.
const SLOW: Duration = Duration::from_secs(6);

#[tokio::test]
async fn a_pool_on_a_server_that_answered_waits_for_a_slow_connection() {
    let upstream = knobas_db::test_util::test_connector().await.url();
    let proxy = SlowAfterFirst::start(&upstream).await;
    let dir = tempfile::tempdir().unwrap();
    // The first contact goes through at once and is the one the bound is for.
    let db = EmbeddedDb::start(DbConfig {
        root_dir: dir.path().to_path_buf(),
        existing_url: Some(proxy.url.clone()),
    })
    .await
    .expect("the first contact is answered at once");

    let connector = db.connector();
    let (pool, app_pool) = tokio::join!(connector.pool(1), connector.app_pool());
    for (which, pool) in [("pool", pool), ("app_pool", app_pool)] {
        let pool = pool.unwrap_or_else(|error| {
            panic!("{which} gave up on a live server answering in {SLOW:?}: {error}")
        });
        let one: i32 = sqlx::query_scalar("select 1")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(one, 1);
        pool.close().await;
    }
    assert!(
        proxy.accepted.load(Ordering::SeqCst) >= 3,
        "the two pools connected through the slow proxy"
    );
    db.stop().await.unwrap();
}

/// A TCP proxy onto `upstream`'s server that relays its first connection at
/// once and holds every later one for [`SLOW`] before relaying it.
struct SlowAfterFirst {
    url: String,
    accepted: Arc<AtomicUsize>,
}

impl SlowAfterFirst {
    async fn start(upstream: &str) -> SlowAfterFirst {
        let target = {
            let url = url::Url::parse(upstream).unwrap();
            format!("{}:{}", url.host_str().unwrap(), url.port().unwrap())
        };
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let port = listener.local_addr().unwrap().port();
        let accepted = Arc::new(AtomicUsize::new(0));
        tokio::spawn({
            let accepted = Arc::clone(&accepted);
            async move {
                loop {
                    let Ok((mut client, _)) = listener.accept().await else {
                        return;
                    };
                    let nth = accepted.fetch_add(1, Ordering::SeqCst);
                    let target = target.clone();
                    tokio::spawn(async move {
                        if nth > 0 {
                            tokio::time::sleep(SLOW).await;
                        }
                        let mut server = tokio::net::TcpStream::connect(target).await.unwrap();
                        let _ = tokio::io::copy_bidirectional(&mut client, &mut server).await;
                    });
                }
            }
        });
        let mut url = url::Url::parse(upstream).unwrap();
        url.set_port(Some(port)).unwrap();
        SlowAfterFirst {
            url: url.to_string(),
            accepted,
        }
    }
}
