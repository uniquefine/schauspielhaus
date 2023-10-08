use diesel::PgConnection;
use log::debug;
use log::error;
use log::info;
use schauspielhaus::establish_connection;
use schauspielhaus::models::create_play_with_screenings;
use tokio::time::{sleep, Duration};

// update_plays fetches the most recent plays from schauspielhaus and updates the database state.
fn update_plays(mut connection: &mut PgConnection) {
    match schauspielhaus::scrape::get_plays() {
        Ok(plays) => {
            info!("Found {} plays, inserting", plays.len());
            for (_url, play) in plays {
                create_play_with_screenings(&mut connection, play).expect("Error creating play");
            }
        }
        Err(e) => {
            error!("Error getting plays: {}", e.to_string());
        }
    }
}

async fn run_sync_function_periodically() {
    loop {
        tokio::task::spawn_blocking(|| {
            info!("establish database connection");
            let connection = &mut establish_connection();
            info!("fetch new plays from schauspielhaus website");
            update_plays(connection);
        })
        .await
        .unwrap();

        // Wait for 3 hours before running the scraper again.
        sleep(Duration::from_secs(60 * 60 * 3)).await;
    }
}

#[tokio::main]
async fn main() {
    let env = env_logger::Env::default().filter_or("RUST_LOG", "info");
    env_logger::init_from_env(env);
    run_sync_function_periodically().await;
}
