#[macro_use]
extern crate rocket;

use askama_rocket::Template;
use rocket::fairing::AdHoc;
use rocket::form::Form;
use rocket::http::Status;
use rocket_db_pools::{Connection, Database, sqlx};
use rocket_db_pools::sqlx::Acquire;
use rocket_db_pools::sqlx::Row;
use rocket_db_pools::sqlx::sqlite::SqliteRow;

#[derive(Database)]
#[database("sqlite_todos")]
struct Todos(sqlx::SqlitePool);

#[derive(sqlx::FromRow)]
struct Todo {
    id: i32,
    description: String,
    completed: bool,
}

#[derive(FromForm)]
struct TodoInput {
    description: String,
}

#[derive(FromForm)]
struct TodoUpdate {
    completed: bool,
}

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate {}

#[rocket::get("/")]
fn index() -> IndexTemplate {
    IndexTemplate {}
}

#[derive(Template)]
#[template(path = "todos.html")]
struct TodosTemplate {
    todos: Vec<Todo>,
}

#[derive(Template)]
#[template(path = "todo.html")]
struct NewTodoTemplate {
    todo: Todo,
}

#[rocket::get("/todos")]
async fn get_todos(mut pool: Connection<Todos>) -> TodosTemplate {
    let mut todos = Vec::new();
    if let Ok(conn) = pool.acquire().await {
        todos = sqlx::query("SELECT * FROM todos")
            .map(|row: SqliteRow| Todo {
                id: row.get(0),
                description: row.get(1),
                completed: row.get(2),
            })
            .fetch_all(conn)
            .await
            .expect("Failed to fetch todos");
    }
    TodosTemplate { todos }
}

#[rocket::delete("/todos/<id>")]
async fn delete_todo(id: i32, mut pool: Connection<Todos>) -> Status {
    if let Ok(conn) = pool.acquire().await {
        sqlx::query("DELETE FROM todos WHERE id = ?")
            .bind(id)
            .execute(conn)
            .await
            .expect("Failed to delete todo");
    }
    Status::Ok
}

#[rocket::put("/todos/<id>", data = "<todo_update>")]
async fn update_todo(id: i32, todo_update: Form<TodoUpdate>, mut pool: Connection<Todos>) -> NewTodoTemplate {
    let mut todo = Todo {
        id,
        description: "".to_string(),
        completed: todo_update.completed,
    };
    if let Ok(conn) = pool.acquire().await {
        sqlx::query("UPDATE todos SET completed = ? WHERE id = ?")
            .bind(todo_update.completed)
            .bind(id)
            .execute(&mut *conn)
            .await
            .expect("Failed to delete todo");
        todo = sqlx::query("SELECT * FROM todos WHERE id = ?")
            .bind(id)
            .map(|row: SqliteRow| Todo {
                id: row.get(0),
                description: row.get(1),
                completed: row.get(2),
            })
            .fetch_one(conn)
            .await
            .expect("Failed to insert todo");
    }
    NewTodoTemplate { todo }
}

#[rocket::post("/todos", data = "<todo_input>")]
async fn create_todo(todo_input: Form<TodoInput>, mut pool: Connection<Todos>) -> NewTodoTemplate {
    let mut todo: Todo = Todo {
        id: 0,
        description: todo_input.description.clone(),
        completed: false,
    };
    if let Ok(conn) = pool.acquire().await {
        sqlx::query("INSERT INTO todos (description) VALUES (?)")
            .bind(todo_input.description.clone())
            .execute(&mut *conn)
            .await
            .expect("Failed to insert todo");
        todo = sqlx::query("SELECT * FROM todos WHERE id = last_insert_rowid()")
            .map(|row: SqliteRow| Todo {
                id: row.get(0),
                description: row.get(1),
                completed: row.get(2),
            })
            .fetch_one(conn)
            .await
            .expect("Failed to insert todo");
    }
    NewTodoTemplate { todo }
}

#[launch]
fn rocket() -> _ {
    rocket::build()
        .attach(Todos::init())
        .attach(AdHoc::try_on_ignite("Database Migrations", |rocket| async {
            let pool = match Todos::fetch(&rocket) {
                Some(pool) => pool.0.clone(),
                None => return Err(rocket),
            };
            sqlx::migrate!("./migrations")
                .run(&pool)
                .await
                .expect("Failed to run migrations");
            Ok(rocket)
        }))
        .mount("/", routes![index, get_todos, create_todo, delete_todo, update_todo])
}