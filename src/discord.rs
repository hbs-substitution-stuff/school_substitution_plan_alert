use serenity::{
	prelude::*,
	framework::standard::{
		CommandResult,
		DispatchError,
		macros::{group, hook, command},
		StandardFramework,
	},
	model::{
		channel::Message,
	},
};
use sqlx::{Pool, Sqlite};
use serenity::client::bridge::gateway::{GatewayIntents, ShardManager};
use std::sync::Arc;
use serenity::http::Http;
use serenity::model::prelude::{UserId, Ready};
use std::collections::{HashMap, HashSet};
use std::io::Write;
use serenity::async_trait;
use serde::{Serialize, Deserialize};
use crate::substitution_pdf_getter::Weekdays;
use std::path::Path;
use serenity::framework::standard::Args;

#[derive(Serialize, Deserialize)]
pub struct ClassesAndUsers {
	classes_and_users: HashMap<String, Vec<u64>>,
}

impl ClassesAndUsers {
	pub fn default() -> Self {
		Self {
			classes_and_users: HashMap::new()
		}
	}

	pub fn new_from_file() -> Self {
		let path = std::env::var("USER_CLASSES_SAVE_LOCATION").expect("Couldn't find the save file location in the environment");
		let path = Path::new(&path);
		if !path.exists() {
			return Self::default();
		}

		let file = std::fs::OpenOptions::new()
			.read(true)
			.write(false)
			.open(path)
			.expect("Couldn't open user file");

		serde_json::from_reader(file).expect("Malformed User Save file")
	}

	//TODO make function Async and use Tokio async file operations
	pub fn write_to_file(&self) {
		let path = std::env::var("USER_CLASSES_SAVE_LOCATION").expect("Couldn't find the save file location in the environment");
		let mut file = std::fs::OpenOptions::new()
			.write(true)
			.create(true)
			.truncate(true)
			.open(path)
			.expect("Couldn't open user save file");
		let json = serde_json::to_string_pretty(self).unwrap();
		if let Err(why) = file.write_all(json.as_bytes()) {
			log::error!("{}", why)
		}
	}

	pub fn insert_user(&mut self, class: String, user_id: u64) {
		self.
			classes_and_users
			.entry(class)
			.or_insert(Vec::new())
			.push(user_id);
		self.write_to_file();
	}

	pub fn get_classes(&self) -> Vec<String> {
		let mut classes = Vec::new();
		for class in self.classes_and_users.keys() {
			classes.push(class.clone());
		}
		classes
	}
}

impl TypeMapKey for ClassesAndUsers {
	type Value = ClassesAndUsers;
}

pub trait Notifier {
	fn notify_users_for_class(&self, class: &str);

	fn get_classes(&self) -> Vec<String>;

	fn insert_user(&mut self, class: String, user_id: u64);
}

pub struct DiscordNotifier {
	http: Arc<Http>,
	data: Arc<RwLock<TypeMap>>,
}

impl DiscordNotifier {
	pub async fn new(token: &str, prefix: &str) -> Self {
		let mut owners = HashSet::new();
		owners.insert(UserId::from(191594115907977225));

		let framework = StandardFramework::new()
			.configure(|c| c
				.with_whitespace(true)
				.on_mention(Some(UserId::from(881938899876868107)))
				.prefix(prefix)
				.delimiters(vec![", ", ",", " "])
				.owners(owners)
			)
			.before(before)
			.after(after)
			.unrecognised_command(unknown_command)
			.normal_message(normal_message)
			.on_dispatch_error(dispatch_error)
			.group(&GENERAL_GROUP);

		let client_builder = Client::builder(token)
			.event_handler(Handler)
			.framework(framework)
			.intents(GatewayIntents::all()); //change to only require the intents we actually want

		let mut client = client_builder.await.expect("Error creating discord client");

		{
			let mut data = client.data.write().await;
			data.insert::<ShardManagerContainer>(Arc::clone(&client.shard_manager));
			data.insert::<ClassesAndUsers>(ClassesAndUsers::new_from_file())
		}

		let http = client.cache_and_http.http.clone();
		let data = client.data.clone();


		tokio::spawn(async move {
			if let Err(why) = client.start().await {
				log::error!("{}", why);
			}
			std::process::exit(69);
		});

		Self {
			http,
			data,
		}
	}

	async fn notify_users_for_class(&self, class: &str, day: Weekdays) -> Result<(), serenity::Error> {
		log::info!("Notifying all users in class {} on day {}", class, day);

		let data = self.data.read().await;
		let classes_and_users = data.get::<ClassesAndUsers>().unwrap();

		for user in classes_and_users.classes_and_users.get(class).unwrap() {
			let user = UserId::from(*user);
			let dm_channel = user.create_dm_channel(&self.http).await?;
			dm_channel.say(&self.http, format!(
				"Es gibt eine Vertretungsplanänderung am {} für Klasse {}",
				day,
				class,
			),
			).await?;//TODO refine, send the link to the corresponding day maybe too etc.
		}

		Ok(())
	}

	//TODO use the classes_and_users function to retrieve classes
	async fn get_classes(&self) -> Vec<String> {
		let mut data = self.data.read().await;
		let classes_and_users = data.get::<ClassesAndUsers>().unwrap();
		classes_and_users.get_classes()
	}

	async fn insert_user(&mut self, class: String, user_id: u64) {
		let mut data = self.data.write().await;
		let classes_and_users = data.get_mut::<ClassesAndUsers>().unwrap();
		classes_and_users.insert_user(class, user_id);
	}
}

#[group]
#[commands(register)]
pub struct General;

#[command]
#[aliases("register_class")]
async fn register(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
	let user = msg.author.id.0;
	let class = args.single::<String>().unwrap();
	if class.len() < 3 {
		let _ = msg.reply_ping(&ctx.http, "Incorrect Arguments").await;
	}

	let mut data = ctx.data.write().await;
	let classes_and_users = data.get_mut::<ClassesAndUsers>().unwrap();
	classes_and_users.insert_user(class.clone(), user);

	msg.reply_ping(&ctx.http, format!("Registered you for class {}", &class)).await;
	log::debug!("Registered {}#{} for class {}", msg.author.name, msg.author.discriminator, &class);

	Ok(())
}




//Fuck this, async traits are hell currently
// #[async_trait]
// impl Notifier for DiscordNotifier {
// 	async fn notify_users_for_class(&self, class: &str) {
// 		todo!()
// 	}
//
// 	async fn get_classes(&self) -> Vec<&str> {
// 		todo!()
// 	}
//
// 	async fn insert_user(&mut self, class: String, user_id: u64) {
// 		let mut data = self.data.write().await;
// 		let classes_and_users = data.get_mut::<ClassesAndUsers>().unwrap();
// 		classes_and_users.insert_user(class, user_id);
// 	}
// }

pub struct ConnectionPool;

impl TypeMapKey for ConnectionPool {
	type Value = Pool<Sqlite>;
}

struct ShardManagerContainer;

impl TypeMapKey for ShardManagerContainer {
	type Value = Arc<Mutex<ShardManager>>;
}

#[hook]
pub async fn before(_ctx: &Context, msg: &Message, command_name: &str) -> bool {
	log::info!("Got command '{}' by user '{}'", command_name, msg.author.name);

	true // if `before` returns false, command processing doesn't happen.
}

#[hook]
pub async fn after(_ctx: &Context, _msg: &Message, command_name: &str, command_result: CommandResult) {
	match command_result {
		Ok(()) => log::info!("Processed command '{}'", command_name),
		Err(why) => log::error!("Command returned an error: {:?}", why),
	}
}

#[hook]
pub async fn unknown_command(ctx: &Context, msg: &Message, unknown_command_name: &str) {
	log::debug!("Could not find command named '{}'\n(Message content: \"{}\")", unknown_command_name, msg.content);
	let reply = msg.channel_id.say(&ctx.http,
								   format!("Sorry, couldn't find a command named '`{}`'\n\n With the `help` command you can list all available commands", unknown_command_name),
	).await;

	if let Err(why) = reply {
		log::error!("Error replying to unknown command: {:?}", why)
	}
}

#[hook]
pub async fn normal_message(_ctx: &Context, msg: &Message) {
	log::info!("Processed non Command message: '{}'", msg.content)
}

#[hook]
pub async fn dispatch_error(ctx: &Context, msg: &Message, error: DispatchError) {
	if let DispatchError::Ratelimited(info) = error {
		// We notify them only once.
		if info.is_first_try {
			let _ = msg
				.channel_id
				.say(&ctx.http, &format!("Try this again in {} seconds.", info.as_secs()))
				.await;
		}
	}
}

pub struct Handler;

#[async_trait]
impl EventHandler for Handler {
	async fn message(&self, ctx: Context, message: Message) {
		log::info!("Message by: {} with content: {}", message.author.name, message.content);
	}

	async fn ready(
		&self,
		_ctx: Context,
		data_about_bot: Ready,
	) {
		log::info!("{} está aqui!", data_about_bot.user.name)
	}
}