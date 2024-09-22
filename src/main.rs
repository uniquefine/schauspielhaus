use std::any::Any;

use anyhow::Context;
use clap::Parser;
use clap::Subcommand;
use diesel::PgConnection;
use dotenvy::dotenv;
use log::debug;
use log::error;
use log::info;
use rand::seq::SliceRandom;
use schauspielhaus::establish_connection;
use schauspielhaus::models::create_play_with_screenings;
use schauspielhaus::models::get_chat;
use schauspielhaus::models::get_chat_with_topics;
use schauspielhaus::models::get_chats;
use schauspielhaus::models::get_play;
use schauspielhaus::models::get_play_for_topic;
use schauspielhaus::models::get_plays_and_topics;
use schauspielhaus::models::get_plays_without_topic;
use schauspielhaus::models::get_screenings;
use schauspielhaus::models::put_chat;
use schauspielhaus::models::put_topic;
use schauspielhaus::models::Chat;
use schauspielhaus::models::ChatWithTopics;
use schauspielhaus::models::PlayAndTopic;
use schauspielhaus::models::PlayWithScreenings;
use schauspielhaus::models::Screening;
use schauspielhaus::models::Topic;
use teloxide::adaptors::throttle::Limits;
use teloxide::adaptors::Throttle;
use teloxide::payloads::SendPollSetters;
use teloxide::types::ParseMode;
use teloxide::RequestError;
use teloxide::{prelude::*, types::ChatKind, utils::command::BotCommands};

use time::macros::format_description;
use time::OffsetDateTime;
use tokio::task;
use tokio::task::spawn_blocking;
use tokio::time::{sleep, Duration};
use url::Url;

#[derive(Parser)]
#[command(name = "schau")]
#[command(about = "Bot to keep track of schauspielhaus plays")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    // Command to start the bot
    #[command(about = "Start the bot")]
    Start,
    // Command to scrape the schauspielhaus website
    #[command(about = "Scrape the schauspielhaus website")]
    Scrape,
    // Command to list plays in the database
    #[command(about = "List plays in the database")]
    List,
    // List all chats in the database
    #[command(about = "List chats in the database")]
    ListChats,
}

#[tokio::main]
async fn main() {
    dotenv().ok();
    let env = env_logger::Env::default().filter_or("RUST_LOG", "info");
    env_logger::init_from_env(env);

    let cli = Cli::parse();

    match cli.command {
        Commands::Start => {
            start_bot().await;
        }
        Commands::Scrape => {
            task::spawn_blocking(|| {
                info!("establish database connection");
                let connection = &mut establish_connection();
                update_plays(connection);
            })
            .await
            .unwrap();
        }
        Commands::List => task::spawn_blocking(|| {
            let connection = &mut establish_connection();
            let plays = get_plays_without_topic(connection, 0).unwrap();
            for (play, screenings) in plays {
                println!("{}: {}", play.id, play.name);
                for screening in screenings {
                    println!("  {}", screening);
                }
            }
        })
        .await
        .unwrap(),
        Commands::ListChats => task::spawn_blocking(|| {
            let connection = &mut establish_connection();
            let chats = get_chats(connection).unwrap();
            for chat in chats {
                println!("{}: {}", chat.id, chat.name);
            }
        })
        .await
        .unwrap(),
    }
}

async fn start_bot() {
    // spawn_blocking(|| {
    //     info!("establish database connection");
    //     let connection = &mut establish_connection();
    //     info!("fetch new plays from schauspielhaus website");
    //     update_plays(connection);
    // })
    // .await
    // .unwrap();

    log::info!("Starting schauspielhaus bot...");
    let bot = Bot::from_env().throttle(Limits::default());

    //let h = spawn_blocking(|| -> anyhow::Result<(PlayWithScreenings, ChatWithTopics)> {
    //    let conn = &mut establish_connection();
    //    let c = get_chat_with_topics(conn, -1001975554335)?;
    //    let p = get_play(conn, c.topics[0].play_id)?;
    //    return Ok((p, c));
    //})
    //.await;
    //let (p, c) = h.unwrap().unwrap();

    //send_play_info(
    //    &bot,
    //    ChatId(-1001975554335),
    //    &p.play,
    //    c.topics[0].message_thread_id,
    //)
    //.await
    //.unwrap();

    // await both futures concurrently
    tokio::select! {
        _ = run_sync_function_periodically() => {},
        _ = Command::repl(bot, answer) => {},
    }
}

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

//#[tokio::main]
//async fn main() {
//    let env = env_logger::Env::default().filter_or("RUST_LOG", "info");
//    env_logger::init_from_env(env);
//    run_sync_function_periodically().await;
//}

/// These commands are supported:
#[derive(BotCommands, Clone)]
#[command(
    rename_rule = "lowercase",
    description = "These commands are supported:"
)]
enum Command {
    /// Display this text.
    #[command(description = "display this help text.")]
    Help,
    /// Start the bot.
    #[command(description = "start the bot.")]
    Start,
    /// Refresh the topics for all plays in the database.
    #[command(description = "recreate the topics for all plays.")]
    Refresh,
    /// Start a poll for this play.
    #[command(description = "(in a play topic) start a poll for this play.")]
    Poll,
}
const HELP: &str = r"This bot only works in public super groups with topics enabled.";

async fn answer(bot: Throttle<Bot>, msg: Message, cmd: Command) -> Result<(), RequestError> {
    debug!(
        "Received message: {:?} thread id: {:?} chat id: {:?}",
        msg, msg.thread_id, msg.chat.id
    );
    let title = msg.chat.title().clone().unwrap_or("").to_string();
    match cmd {
        Command::Help => {
            bot.send_message(msg.chat.id, Command::descriptions().to_string())
                .await?
        }
        Command::Start => {
            match msg.chat.kind {
                ChatKind::Private { .. } => {
                    bot.send_message(msg.chat.id, HELP).await?;
                    return Ok(());
                }
                ChatKind::Public(p) => match p.kind {
                    teloxide::types::PublicChatKind::Supergroup(s) => {
                        if !s.is_forum {
                            bot.send_message(msg.chat.id, HELP).await?;
                            return Ok(());
                        }
                    }
                    _ => {
                        bot.send_message(msg.chat.id, "HELP").await?;
                        return Ok(());
                    }
                },
            }
            bot.send_message(msg.chat.id, "Let's start the party!")
                .await
                .expect("Sending welcome message failed");

            let mut connection = &mut establish_connection();

            let res = put_chat(
                &mut connection,
                Chat {
                    id: msg.chat.id.0,
                    name: title,
                },
            );

            match res {
                Ok(_) => {
                    bot.send_message(msg.chat.id, "Chat added to database")
                        .await
                        .expect("Error sending message");
                }
                Err(e) => {
                    error! {"Error adding chat {} to database: {}", msg.chat.id.0, e};
                    bot.send_message(msg.chat.id, format!("Error adding chat to database: {}", e))
                        .await
                        .expect("Error sending message");
                    return Ok(());
                }
            }
            if let Err(e) = refresh_topics(&bot, msg.chat.id).await {
                error!("Error refreshing topics for chat {}: {}", msg.chat.id.0, e);
                bot.send_message(
                    msg.chat.id,
                    format!(
                        "Error refreshing topics, please contact the bot admin:\n```{}```",
                        e
                    ),
                )
                .await?;
            }

            return Ok(());
        }
        Command::Refresh => {
            if !ensure_chat_exists(&bot, msg.chat.id).await {
                return Ok(());
            }
            if let Err(e) = refresh_topics(&bot, msg.chat.id).await {
                error!("Error refreshing topics for chat {}: {}", msg.chat.id.0, e);
                bot.send_message(
                    msg.chat.id,
                    format!(
                        "Error refreshing topics, please contact the bot admin:\n```{}```",
                        e
                    ),
                )
                .await?;
            }
            return Ok(());
        }
        Command::Poll => {
            if !ensure_chat_exists(&bot, msg.chat.id).await {
                return Ok(());
            }
            match msg.thread_id {
                None => {
                    bot.send_message(msg.chat.id, "Please use this command in a play topic")
                        .await?;
                    return Ok(());
                }
                Some(topic_id) => {
                    post_poll_for_topic(&bot, msg.chat.id, topic_id).await?;
                }
            }
            return Ok(());
        }
    };

    Ok(())
}

async fn post_poll_for_topic(
    bot: &Throttle<Bot>,
    msg_chat_id: ChatId,
    topic_id: i32,
) -> Result<(), RequestError> {
    let connection = &mut establish_connection();
    let play_with_screenings = match get_play_for_topic(connection, topic_id) {
        Ok(p) => p,
        Err(diesel::result::Error::NotFound) => {
            bot.send_message(msg_chat_id, "No play found for this topic.")
                .reply_to_message_id(teloxide::types::MessageId(topic_id))
                .await?;
            return Ok(());
        }
        Err(e) => {
            error!("Error getting play for topic {}: {}", topic_id, e);
            bot.send_message(
                msg_chat_id,
                format!(
                    "Unexpected error getting play for topic {}: {}",
                    topic_id, e
                ),
            )
            .reply_to_message_id(teloxide::types::MessageId(topic_id))
            .await?;
            return Ok(());
        }
    };
    // only consider screenings in the future
    let now = OffsetDateTime::now_utc();
    let screenings = play_with_screenings
        .screenings
        .iter()
        .filter(|s| s.start_time > now)
        .collect::<Vec<&Screening>>();
    let total = screenings.len() / 10;
    for (i, chunk) in screenings.chunks(10).enumerate() {
        let title = match total > 0 {
            true => format!("When should we go? {}/{}", i, total),
            false => "When should we go?".to_string(),
        };
        bot.send_poll(msg_chat_id, title, chunk.iter().map(|s| option(s)))
            .reply_to_message_id(teloxide::types::MessageId(topic_id))
            .allows_multiple_answers(true)
            .is_anonymous(false)
            .await?;
    }
    return Ok(());
}

fn option(s: &&Screening) -> String {
    let format = format_description!("[weekday] [day].[month].[year] [hour]:[minute]");
    s.start_time.format(&format).unwrap()
}

async fn ensure_chat_exists(bot: &Throttle<Bot>, msg_chat_id: ChatId) -> bool {
    let connection = &mut establish_connection();
    let chat = get_chat(connection, msg_chat_id.0);
    match chat {
        Ok(_) => {
            return true;
        }
        Err(_) => {
            let _ = bot
                .send_message(
                    msg_chat_id,
                    "Chat not found in database, please use /start first",
                )
                .await
                .map_err(|e| {
                    error!("Error sending message to chat: {}", e.to_string());
                    e
                });
            return false;
        }
    }
}

async fn random_forum_icon(bot: &Throttle<Bot>) -> Result<Option<String>, RequestError> {
    let stickers = bot.get_forum_topic_icon_stickers().await?;
    let sticker = stickers.choose(&mut rand::thread_rng());
    Ok(sticker
        .map(|s| s.custom_emoji_id())
        .flatten()
        .map(|e| e.to_string()))
}

async fn refresh_topics(bot: &Throttle<Bot>, msg_chat_id: ChatId) -> Result<(), anyhow::Error> {
    let mut connection = &mut establish_connection();
    let plays = get_plays_and_topics(connection, msg_chat_id.0).map_err(|e| {
        error!("Error getting plays: {}", e.to_string());
        e
    })?;
    // collect errors
    let mut errors = vec![];
    for PlayAndTopic {
        play: PlayWithScreenings { play, screenings },
        topic,
    } in plays
    {
        let message_thread_id = match &topic {
            Some(t) => t.message_thread_id,
            None => {
                // Create the forum topic
                let icon = random_forum_icon(&bot).await?.unwrap_or("".to_string());
                let t = match bot
                    .create_forum_topic(msg_chat_id, &play.name, 0, icon)
                    .await
                {
                    Ok(t) => t,
                    Err(e) => {
                        errors.push(anyhow::Error::msg(format!(
                            "Error creating topic for play '{}': {}",
                            play.name, e
                        )));
                        continue;
                    }
                };
                debug!("Created topic {} with ID {}", t.name, t.message_thread_id);
                t.message_thread_id
            }
        };
        // Delete the existing pinned message
        if let Some(t) = &topic {
            match bot
                .delete_message(msg_chat_id, teloxide::types::MessageId(t.pinned_message_id))
                .await
            {
                Ok(_) => {}
                Err(e) => {
                    errors.push(anyhow::Error::msg(format!(
                        "Error deleting pinned message for play '{}': {}",
                        play.name, e
                    )));
                    // Don't continue here if e.g. the message was already deleted
                    // we still want to send the new message.
                }
            }
        }

        let pinned_message_id =
            match send_play_info(&bot, msg_chat_id, &play, &screenings, message_thread_id).await {
                Ok(id) => id,
                Err(e) => {
                    errors.push(anyhow::Error::msg(format!(
                        "Error sending play info for play '{}': {}",
                        play.name, e
                    )));
                    continue;
                }
            };
        match put_topic(
            &mut connection,
            Topic {
                message_thread_id: message_thread_id,
                play_id: play.id,
                chat_id: msg_chat_id.0,
                pinned_message_id: pinned_message_id,
                last_updated: OffsetDateTime::now_utc(),
            },
        ) {
            Ok(_) => {}
            Err(e) => {
                errors.push(anyhow::Error::msg(format!(
                    "Error saving topic for play '{}' in database: {}",
                    play.name, e
                )));
                continue;
            }
        };
    }
    if errors.len() > 0 {
        return Err(anyhow::Error::msg(format!(
            "Errors refreshing topics: {}",
            errors
                .iter()
                .map(|e| e.to_string())
                .collect::<Vec<String>>()
                .join(", ")
        )));
    }
    Ok(())
}

async fn send_play_info(
    bot: &Throttle<Bot>,
    msg_chat_id: ChatId,
    play: &schauspielhaus::models::Play,
    screenings: &Vec<schauspielhaus::models::Screening>,
    message_thread_id: i32,
) -> Result<i32, RequestError> {
    let mut message_text = format!(
        "\
🎭 [*{}*]({}{})

{}

{}
",
        play.name,
        schauspielhaus::scrape::BASE_URL,
        play.url,
        play.description,
        play.meta_info,
    );
    if screenings.len() > 0 {
        message_text.push_str("\n🎟️ *Screenings*:");
    }
    for screening in screenings {
        message_text.push_str(&format!(
            "\n- {} [tickets]({})",
            option(&screening),
            format!(
                "https://www.zurichticket.ch/shz.webshop/webticket/shop?event={}",
                screening
                    .webid
                    .trim_start_matches("event_")
                    .trim_end_matches("@www.schauspielhaus.ch"),
            )
        ));
    }
    let pinned_msg = bot
        .send_message(msg_chat_id, message_text)
        .parse_mode(ParseMode::MarkdownV2)
        .reply_to_message_id(teloxide::types::MessageId(message_thread_id))
        .await?;

    // TODO: send screenigns
    Ok(pinned_msg.id.0)
}

async fn run_sync_function_periodically() {
    loop {
        // Wait for 3 hours before running the scraper.
        sleep(Duration::from_secs(60 * 60 * 3)).await;
        tokio::task::spawn_blocking(|| {
            info!("establish database connection");
            let connection = &mut establish_connection();
            info!("fetch new plays from schauspielhaus website");
            update_plays(connection);
        })
        .await
        .unwrap();
    }
}
