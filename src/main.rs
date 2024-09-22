use std::hash::Hash;
use std::hash::Hasher;

use clap::Parser;
use clap::Subcommand;
use diesel::PgConnection;
use dotenvy::dotenv;
use env_logger;
use log::debug;
use log::error;
use log::info;
use log::LevelFilter;
use rand::seq::SliceRandom;
use rand::Rng;
use schauspielhaus::establish_connection;
use schauspielhaus::models::create_play_with_screenings;
use schauspielhaus::models::get_chat;
use schauspielhaus::models::get_chats;
use schauspielhaus::models::get_play_for_topic;
use schauspielhaus::models::get_plays_and_topics;
use schauspielhaus::models::get_plays_without_topic;
use schauspielhaus::models::put_chat;
use schauspielhaus::models::put_topic;
use schauspielhaus::models::Chat;
use schauspielhaus::models::PlayAndTopic;
use schauspielhaus::models::PlayWithScreenings;
use schauspielhaus::models::Screening;
use schauspielhaus::models::Topic;
use teloxide::adaptors::throttle::Limits;
use teloxide::adaptors::Throttle;
use teloxide::payloads::SendPollSetters;
use teloxide::types::Me;
use teloxide::types::ParseMode;
use teloxide::utils::markdown;
use teloxide::ApiError;
use teloxide::RequestError;
use teloxide::{prelude::*, types::ChatKind, utils::command::BotCommands};

use time::macros::format_description;
use time::OffsetDateTime;
use tokio::task;
use tokio::time::{sleep, Duration};

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
            info!("establish database connection");
            let connection = &mut establish_connection();
            update_plays(connection).await;
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
    log::info!("Starting schauspielhaus bot...");
    let bot = Bot::from_env().throttle(Limits::default());

    // await both futures concurrently
    tokio::select! {
        _ = Command::repl(bot.clone(), answer) => {},
       _ = run_sync_function_periodically(&bot) => {},
    }
}

// update_plays fetches the most recent plays from schauspielhaus and updates the database state.
async fn update_plays(mut connection: &mut PgConnection) {
    match schauspielhaus::scrape::get_plays().await {
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
    /// Force a refresh of the topics for all plays in the database.
    #[command(description = "force recreate the topics for all plays.")]
    ForceRefresh,
    /// Start a poll for this play.
    #[command(description = "(in a play topic) start a poll for this play.")]
    Poll,
}
const HELP: &str = r"This bot only works in public super groups with topics enabled.";

async fn answer(bot: Throttle<Bot>, msg: Message, cmd: Command) -> ResponseResult<()> {
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
            match refresh_topics(&bot, msg.chat.id, false).await {
                Ok(_) => {
                    bot.send_message(msg.chat.id, "Topics created")
                        .await
                        .expect("Error sending message");
                }
                Err(e) => {
                    error!("Error refreshing topics for chat {}: {}", msg.chat.id.0, e);
                    bot.send_message(
                        msg.chat.id,
                        format!(
                            "Error refreshing topics, please contact the bot admin:\n```{}```",
                            e
                        ),
                    )
                    .await
                    .expect("Error sending message");
                }
            }
            return Ok(());
        }
        Command::Refresh => {
            if !ensure_chat_exists(&bot, msg.chat.id).await {
                return Ok(());
            }
            match refresh_topics(&bot, msg.chat.id, false).await {
                Ok(_) => {
                    bot.send_message(msg.chat.id, "Topics refreshed")
                        .await
                        .expect("Error sending message");
                }
                Err(e) => {
                    error!("Error refreshing topics for chat {}: {}", msg.chat.id.0, e);
                    bot.send_message(msg.chat.id, format!("Error refreshing topics: {}", e))
                        .await?;
                }
            }
            return Ok(());
        }
        Command::ForceRefresh => {
            if !ensure_chat_exists(&bot, msg.chat.id).await {
                return Ok(());
            }
            match refresh_topics(&bot, msg.chat.id, true).await {
                Ok(_) => {
                    bot.send_message(msg.chat.id, "Topics refreshed")
                        .await
                        .expect("Error sending message");
                }
                Err(e) => {
                    error!("Error refreshing topics for chat {}: {}", msg.chat.id.0, e);
                    bot.send_message(msg.chat.id, format!("Error refreshing topics: {}", e))
                        .await?;
                }
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
    topic_id: teloxide::types::ThreadId,
) -> Result<(), RequestError> {
    let connection = &mut establish_connection();
    let play_with_screenings = match get_play_for_topic(connection, topic_id.0 .0) {
        Ok(p) => p,
        Err(diesel::result::Error::NotFound) => {
            bot.send_message(msg_chat_id, "No play found for this topic.")
                .message_thread_id(topic_id)
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
            .message_thread_id(topic_id)
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
            .message_thread_id(topic_id)
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

// pick one at random from 0x6FB9F0, 0xFFD67E, 0xCB86DB, 0x8EEE98, 0xFF93B2, or 0xFB6F5F
fn random_icon_color() -> u32 {
    let colors = [0x6FB9F0, 0xFFD67E, 0xCB86DB, 0x8EEE98, 0xFF93B2, 0xFB6F5F];
    *colors.choose(&mut rand::thread_rng()).unwrap()
}

async fn refresh_topics(
    bot: &Throttle<Bot>,
    msg_chat_id: ChatId,
    force: bool,
) -> Result<(), anyhow::Error> {
    let mut connection = &mut establish_connection();
    let plays = get_plays_and_topics(connection, msg_chat_id.0).map_err(|e| {
        error!("Error getting plays: {}", e.to_string());
        e
    })?;
    debug!("Found {} plays to refresh", plays.len());
    // collect errors
    let mut errors = vec![];
    for PlayAndTopic {
        play: PlayWithScreenings { play, screenings },
        topic,
    } in plays
    {
        // break if errors is not empty
        if errors.len() > 0 {
            break;
        }

        let message_thread_id = match &topic {
            Some(t) => teloxide::types::ThreadId(teloxide::types::MessageId(t.message_thread_id)),
            None => {
                // Create the forum topic
                let t = match bot
                    .create_forum_topic(msg_chat_id, &play.name, random_icon_color(), "")
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
                debug!("Created topic {} with ID {}", t.name, t.thread_id);
                t.thread_id
            }
        };
        // Delete the existing pinned message
        let mut pinned_message_id = topic.as_ref().map_or(0, |t| t.pinned_message_id);
        let pinned_message_hash = topic.as_ref().map_or(0, |t| t.pinned_message_hash);

        let message_text = pinned_message(&play, &screenings);
        let message_hash = (|| {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            message_text.hash(&mut hasher);
            hasher.finish()
        })();
        if force || (pinned_message_hash as u64) != message_hash {
            if pinned_message_id != 0 {
                // The message has changed, update it
                match bot
                    .delete_message(msg_chat_id, teloxide::types::MessageId(pinned_message_id))
                    .await
                {
                    Ok(_) => {}
                    Err(RequestError::Api(ApiError::MessageToDeleteNotFound)) => {
                        // ignore if the message is already deleted
                    }
                    Err(e) => {
                        errors.push(anyhow::Error::msg(format!(
                            "Error deleting pinned message for play '{}': {}",
                            play.name, e
                        )));
                        continue;
                    }
                }
            }
            pinned_message_id =
                match create_pinned_message(&bot, message_text, msg_chat_id, message_thread_id)
                    .await
                {
                    Ok(id) => id,
                    Err(RequestError::Api(ApiError::MessageToReplyNotFound)) => {
                        // ignore if the topic was deleted
                        continue;
                    }
                    Err(e) => {
                        errors.push(anyhow::Error::msg(format!(
                            "Error sending play info for play '{}': {}",
                            play.name, e
                        )));
                        continue;
                    }
                };
        }

        match put_topic(
            &mut connection,
            Topic {
                message_thread_id: message_thread_id.0 .0,
                play_id: play.id,
                chat_id: msg_chat_id.0,
                pinned_message_id: pinned_message_id,
                pinned_message_hash: message_hash as i64,
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

fn pinned_message(
    play: &schauspielhaus::models::Play,
    screenings: &Vec<schauspielhaus::models::Screening>,
) -> String {
    let mut message_text = format!(
        "\
[*{}*]({}{}) 🎭️

{}

{}
",
        markdown::escape(&play.name),
        schauspielhaus::scrape::BASE_URL,
        play.url,
        markdown::escape(&play.description),
        markdown::escape(&play.meta_info),
    );
    if screenings.len() > 0 {
        message_text.push_str("\n🎟️ *Screenings*:");
    }
    for screening in screenings {
        message_text.push_str(&format!(
            "\n\\- {} [tickets]({})",
            markdown::escape(&option(&screening)),
            format!(
                "https://www.zurichticket.ch/shz.webshop/webticket/shop?event={}",
                screening
                    .webid
                    .trim_start_matches("event_")
                    .trim_end_matches("@www.schauspielhaus.ch"),
            )
        ));
    }
    message_text
}

async fn create_pinned_message(
    bot: &Throttle<Bot>,
    message_text: String,
    msg_chat_id: ChatId,
    msg_thread_id: teloxide::types::ThreadId,
) -> Result<i32, RequestError> {
    let pinned_msg = bot
        .send_message(msg_chat_id, message_text)
        .parse_mode(ParseMode::MarkdownV2)
        .message_thread_id(msg_thread_id)
        .await?;

    // TODO: send screenigns
    Ok(pinned_msg.id.0)
}

async fn run_sync_function_periodically(bot: &Throttle<Bot>) {
    loop {
        info!("establish database connection");
        let connection = &mut establish_connection();
        info!("fetch new plays from schauspielhaus website");
        update_plays(connection).await;
        let chats = get_chats(&mut establish_connection()).unwrap();
        for chat in chats {
            let chat_id = teloxide::prelude::ChatId(chat.id);
            match refresh_topics(bot, chat_id, false).await {
                Ok(_) => {
                    match bot
                        .send_message(chat_id, "Topics refreshed")
                        .disable_notification(true)
                        .await
                    {
                        Ok(_) => {}
                        // ignore if the chat was deleted
                        Err(RequestError::Api(ApiError::ChatNotFound)) => {}
                        Err(e) => {
                            error!("Error sending message to chat {}: {}", chat.id, e);
                        }
                    }
                }
                Err(e) => {
                    match bot
                        .send_message(chat_id, format!("Error refreshing topics: {}", e))
                        .await
                    {
                        Ok(_) => {}
                        // ignore if the chat was deleted
                        Err(RequestError::Api(ApiError::ChatNotFound)) => {}
                        Err(send_err) => {
                            error!(
                                "Error sending message to chat {}: {}, refresh_error: {}",
                                chat.id, send_err, e
                            );
                        }
                    }
                }
            }
        }
        sleep(Duration::from_secs(60 * 60 * 3)).await;
    }
}

#[test]
fn test_get_plays_and_topics() {
    let _ = env_logger::builder()
        .is_test(true)
        .filter_level(LevelFilter::Debug)
        .try_init();
    let mut connection = &mut establish_connection();
    println!("Fetching plays and topics");
    let chats = get_chats(&mut connection).unwrap();
    for chat in chats {
        let plays = get_plays_and_topics(&mut connection, chat.id).unwrap();
        for PlayAndTopic { play, topic } in plays {
            match topic {
                Some(t) => println!("{}: {}", play.play.name, t.message_thread_id),
                None => println!("{}: no topic", play.play.name),
            }
        }
    }
}
