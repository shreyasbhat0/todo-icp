use candid::CandidType;
use core::cell::{Cell, RefCell};
use ic_cdk::{query, update};
use serde::Deserialize;
use std::collections::HashMap;

// Define a constant for the default page size for pagination
const DEFAULT_PAGE_SIZE: usize = 10;

type TodoStore = HashMap<u64, Todo>;
type TodoOrder = Vec<u64>; // To Maintain Order of TODOS

// Thread-local storage for the Todo state and order
thread_local! {
    static TODOSTATE: RefCell<TodoStore> = RefCell::default();
    static TODOORDER: RefCell<TodoOrder> = RefCell::new(Vec::new());
    static ID: Cell<u64> = Cell::new(0);
}

// Define the Todo data structure
#[derive(CandidType, Deserialize, Default, Clone, Debug)]
struct Todo {
    name: String,
    description: String,
    is_completed: bool,
}

// Implement creation of a Todo item
#[update(name = "create_todo")]
fn create_todo(name: String, description: String) -> Result<u64, String> {
    let next_id = ID.with(|nid| {
        let current = nid.get();
        nid.set(current + 1);
        current
    });

    TODOSTATE.with(|todo_store| {
        todo_store.borrow_mut().insert(
            next_id,
            Todo {
                name,
                description,
                is_completed: false,
            },
        )
    });
    TODOORDER.with(|todo_order| {
        todo_order.borrow_mut().push(next_id.clone());
    });
    Ok(next_id)
}

// Implement retrieval of a single Todo item by ID
#[query(name = "get_todo")]
fn get_todo(todo_id: u64) -> Result<Todo, String> {
    TODOSTATE.with(|todo_store| match todo_store.borrow().get(&todo_id) {
        Some(todo) => Ok(todo.clone()),
        None => Err("Todo Invalid Id".to_string()),
    })
}

// Implement retrieval of Todos with pagination
#[query(name = "get_todos")]
fn get_todos(page: u64, page_size: Option<u64>) -> Vec<Todo> {
    // Determine the actual page size (use default if None provided)
    let page_size = page_size.unwrap_or(DEFAULT_PAGE_SIZE as u64);
    // Calculate start index based on page number
    let start_index = ((page.saturating_sub(1)) * page_size) as usize;

    let todos = TODOORDER.with(|todo_order| {
        let order = todo_order.borrow();
        let end_index = usize::min(start_index + page_size as usize, order.len());

        order[start_index..end_index]
            .iter()
            .filter_map(|id| TODOSTATE.with(|todos| todos.borrow().get(id).cloned()))
            .collect::<Vec<Todo>>()
    });

    todos
}

// Implement updating a Todo item
#[update(name = "update_todo")]
fn update_todo(
    todo_id: u64,
    name: Option<String>,
    description: Option<String>,
    is_completed: Option<bool>,
) -> Result<bool, String> {
    let exists = TODOORDER.with(|order| order.borrow().contains(&todo_id));

    if !exists {
        return Err("Todo not found".to_string());
    }

    TODOSTATE.with(|todo_store| {
        if let Some(todo) = todo_store.borrow_mut().get_mut(&todo_id) {
            if let Some(new_name) = name {
                todo.name = new_name;
            }
            if let Some(new_description) = description {
                todo.description = new_description;
            }
            if let Some(completed) = is_completed {
                todo.is_completed = completed;
            }

            Ok(true)
        } else {
            Err("Todo not found in the store".to_string())
        }
    })
}

// Implement deletion of a Todo item
#[update(name = "delete_todo")]
fn delete_todo(todo_id: u64) -> Result<bool, String> {
    // Attempt to remove the Todo from the store
    let removed = TODOSTATE.with(|todo_store| todo_store.borrow_mut().remove(&todo_id));

    if removed.is_some() {
        TODOORDER.with(|todo_order| {
            let mut order = todo_order.borrow_mut();
            if let Some(pos) = order.iter().position(|id| id == &todo_id) {
                order.remove(pos);
                Ok(true) // Indicate successful deletion
            } else {
                Ok(false) // The item was not found in the order list, indicating inconsistency
            }
        })
    } else {
        // The item was not found in the store
        Err("Todo not found".to_string())
    }
}

ic_cdk::export_candid!();

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_and_get_todo() {
        let name = "Test Todo".to_string();
        let description = "This is a test todo item".to_string();

        // Test creating a todo item
        let id_result = create_todo(name.clone(), description.clone());
        assert!(id_result.is_ok());
        let id = id_result.unwrap();

        // Test getting the created todo item
        let fetched_todo_result = get_todo(id.clone());
        assert!(fetched_todo_result.is_ok());
        let fetched_todo = fetched_todo_result.unwrap();
        assert_eq!(fetched_todo.name, name);
        assert_eq!(fetched_todo.description, description);
        assert!(!fetched_todo.is_completed);
    }

    #[test]
    fn test_update_todo() {
        let name = "Update Test".to_string();
        let description = "This todo will be updated".to_string();
        let updated_name = "Updated Name".to_string();
        let updated_description = "Updated Description".to_string();

        // Create a todo to update
        let id = create_todo(name, description).unwrap();

        // Update the todo
        let update_result = update_todo(
            id.clone(),
            Some(updated_name.clone()),
            Some(updated_description.clone()),
            Some(true),
        );
        assert!(update_result.is_ok());

        // Fetch and verify the updated todo
        let updated_todo = get_todo(id).unwrap();
        assert_eq!(updated_todo.name, updated_name);
        assert_eq!(updated_todo.description, updated_description);
        assert!(updated_todo.is_completed);
    }

    #[test]
    fn test_pagination() {
        TODOSTATE.with(|ts| ts.borrow_mut().clear());

        // Create multiple todos
        for i in 0..25 {
            create_todo(format!("Paginated Todo {i}"), "Description".to_string()).unwrap();
        }

        // Fetch the first page
        let first_page = get_todos(1, Some(10));
        assert_eq!(first_page.len(), 10);

        // Fetch the second page
        let second_page = get_todos(2, Some(10));
        assert_eq!(second_page.len(), 10);

        // Fetch a third page
        let third_page = get_todos(3, Some(10));
        assert_eq!(third_page.len(), 5);
    }

    #[test]
    fn test_delete_todo() {
        let name = "Delete Test".to_string();
        let description = "This todo will be deleted".to_string();

        // First, create a Todo to ensure the application is in a known state
        let id = create_todo(name.clone(), description.clone()).unwrap();

        // Verify the Todo was created successfully
        let fetched_todo = get_todo(id.clone()).unwrap();
        assert_eq!(fetched_todo.name, name);
        assert_eq!(fetched_todo.description, description);
        assert!(!fetched_todo.is_completed);

        // Now, delete the created Todo
        let delete_result = delete_todo(id.clone()).unwrap();
        assert!(
            delete_result,
            "Expected the todo to be successfully deleted"
        );

        // Try to fetch the deleted Todo, expecting an error
        let fetched_after_delete = get_todo(id.clone());
        assert!(
            fetched_after_delete.is_err(),
            "Expected an error when fetching a deleted Todo"
        );

        // Additionally, ensure the Todo ID is no longer in the order list
        let exists_in_order = TODOORDER.with(|todo_order| todo_order.borrow().contains(&id));
        assert!(
            !exists_in_order,
            "Expected the Todo ID to be removed from the order list"
        );
    }
}
