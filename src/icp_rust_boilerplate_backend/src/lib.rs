#[macro_use]
extern crate serde;
use candid::{Decode, Encode};
use ic_cdk::api::time;
use ic_stable_structures::memory_manager::{MemoryId, MemoryManager, VirtualMemory};
use ic_stable_structures::{BoundedStorable, Cell, DefaultMemoryImpl, StableBTreeMap, Storable};
use std::{borrow::Cow, cell::RefCell};

type Memory = VirtualMemory<DefaultMemoryImpl>;
type IdCell = Cell<u64, Memory>;

#[derive(candid::CandidType, Clone, Serialize, Deserialize, Default)]
enum Category {
    #[default]
    Bakery,
    Cake,
    Cookies,
}

#[derive(candid::CandidType, Clone, Serialize, Deserialize, Default)]
struct Product {
    id: u64,
    name: String,
    category: Category,
    quantity: u32,
    created_at: u64,
    updated_at: Option<u64>,
}

// a trait that must be implemented for a struct that is stored in a stable struct
impl Storable for Product {
    fn to_bytes(&self) -> std::borrow::Cow<[u8]> {
        Cow::Owned(Encode!(self).unwrap())
    }

    fn from_bytes(bytes: std::borrow::Cow<[u8]>) -> Self {
        Decode!(bytes.as_ref(), Self).unwrap()
    }
}

// another trait that must be implemented for a struct that is stored in a stable struct
impl BoundedStorable for Product {
    const MAX_SIZE: u32 = 1024;
    const IS_FIXED_SIZE: bool = false;
}

thread_local! {
    static MEMORY_MANAGER: RefCell<MemoryManager<DefaultMemoryImpl>> = RefCell::new(
        MemoryManager::init(DefaultMemoryImpl::default())
    );

    static ID_COUNTER: RefCell<IdCell> = RefCell::new(
        IdCell::init(MEMORY_MANAGER.with(|m| m.borrow().get(MemoryId::new(0))), 0)
            .expect("Cannot create a counter")
    );

    static STORAGE: RefCell<StableBTreeMap<u64, Product, Memory>> =
        RefCell::new(StableBTreeMap::init(
            MEMORY_MANAGER.with(|m| m.borrow().get(MemoryId::new(1)))
    ));
}

#[derive(candid::CandidType, Serialize, Deserialize, Default)]
struct ProductPayload {
    name: String,
    quantity: u32,
    category: Category
}

#[derive(candid::CandidType, Serialize, Deserialize, Default)]
struct StockPayload {
    amount: u32,
}

fn _get_product(id: &u64) -> Option<Product> {
    STORAGE.with(|service| service.borrow().get(id))
}

#[ic_cdk::query]
fn get_product(id: u64) -> Result<Product, Error> {
    match _get_product(&id) {
        Some(product) => Ok(product),
        None => Err(Error::NotFound {
            msg: format!("a product with id={} not found", id),
        }),
    }
}

fn do_insert(product: &Product) {
    STORAGE.with(|service| service.borrow_mut().insert(product.id, product.clone()));
}

#[ic_cdk::update]
fn add_product(product: ProductPayload) -> Option<Product> {
    let id = ID_COUNTER
        .with(|counter| {
            let current_value = *counter.borrow().get();
            counter.borrow_mut().set(current_value + 1)
        })
        .expect("cannot increment id counter");
        
    let item = Product {
        id,
        name: product.name,
        category: product.category, 
        quantity: product.quantity,
        created_at: time(),
        updated_at: None,
    };
    do_insert(&item);
    Some(item)
}

#[ic_cdk::update]
fn update_product(id: u64, payload: ProductPayload) -> Result<Product, Error> {
    match STORAGE.with(|service| service.borrow().get(&id)) {
        Some(mut product) => {
            product.name = payload.name;
            product.category = payload.category;
            product.quantity = payload.quantity;
            product.updated_at = Some(time());
            do_insert(&product);
            Ok(product)
        }
        None => Err(Error::NotFound {
            msg: format!(
                "couldn't update a product with id={}. message not found",
                id
            ),
        }),
    }
}

#[ic_cdk::update]
fn add_quantity(id: u64, payload: StockPayload) -> Result<Product, Error> {
    match STORAGE.with(|service| service.borrow().get(&id)) {
        Some(mut product) => {
            product.quantity += payload.amount;
            product.updated_at = Some(time());
            do_insert(&product);
            Ok(product)
        }
        None => Err(Error::NotFound {
            msg: format!(
                "couldn't add amount a product with id={}. message not found",
                id
            ),
        }),
    }
}

#[ic_cdk::update]
fn offload_quantity(id: u64, payload: StockPayload) -> Result<Product, Error> {
    match STORAGE.with(|service| service.borrow().get(&id)) {
        Some(mut product) => {
            if product.quantity == 0 {
                return Err(Error::InvalidOperation {
                    msg: format!(
                        "Product with id={} cannot be offloaded because the quantity is 0",
                        id
                    ),
                });
            } else if payload.amount > product.quantity {
                return Err(Error::InvalidOperation {
                    msg: format!(
                        "Cannot offload more than the available quantity. Available: {}, Trying to offload: {}",
                        product.quantity, payload.amount
                    ),
                });
            }

            product.quantity -= payload.amount;
            product.updated_at = Some(time());
            do_insert(&product);
            Ok(product)
        }
        None => Err(Error::NotFound {
            msg: format!(
                "couldn't offload a product with id={}. message not found",
                id
            ),
        }),
    }
}


#[ic_cdk::update]
fn remove_product(id: u64) -> Result<Product, Error> {
    match STORAGE.with(|service| service.borrow_mut().remove(&id)) {
        Some(product) => Ok(product),
        None => Err(Error::NotFound {
            msg: format!(
                "couldn't delete a product with id={}. message not found.",
                id
            ),
        }),
    }
}

#[derive(candid::CandidType, Deserialize, Serialize)]
enum Error {
    NotFound { msg: String },
    InvalidOperation { msg: String },
}

// need this to generate candid
ic_cdk::export_candid!();