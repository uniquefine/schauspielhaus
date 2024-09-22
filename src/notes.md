## Goal state

There is a mode to dispatch messages based on some filter criteria. This allows basically saving something like a router for messages. 

What I want to do is have a command to start posting the bots.

Start:
- If chat is not a forum: -> reply with error and return
- If chat is forum: trigger "update topics"
- Add chat ID into database as list of chats to update

Update Topics:
For each chat in the database:
- Find all plays that have not been posted to this chat yet.
- Post them to the chat, each one in their own topic. store the topic id:i32, play_id in the database (foreign key releationship)

triple


periodically call update topics (after scraping the website)

Inside the topic add commands. The commands need to identify the play from the topic -> save (play_id, chat_id, topic_id) in the database.
The following commands are needed (in order of implementation priority):
- /survey -> post a survey with the screenings (bonus don't include ones in the past)
- /close -> close the topic (replace the topic icon with a checkmark)


Additional features:
- Clean up topics for plays that don't have any screenings in the future.

## 15.09.2024
- [ ] Check if scraper still works
