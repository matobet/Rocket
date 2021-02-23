use crate::http::{RawStr, Status};
use crate::request::{Request, local_cache};
use crate::data::{Data, Limits};
use crate::outcome::{self, IntoOutcome, Outcome::*};

/// Type alias for the `Outcome` of [`FromData`] and [`FromData`].
///
/// [`FromData`]: crate::data::FromData
pub type Outcome<S, E> = outcome::Outcome<S, (Status, E), Data>;

impl<S, E> IntoOutcome<S, (Status, E), Data> for Result<S, E> {
    type Failure = Status;
    type Forward = Data;

    #[inline]
    fn into_outcome(self, status: Status) -> Outcome<S, E> {
        match self {
            Ok(val) => Success(val),
            Err(err) => Failure((status, err))
        }
    }

    #[inline]
    fn or_forward(self, data: Data) -> Outcome<S, E> {
        match self {
            Ok(val) => Success(val),
            Err(_) => Forward(data)
        }
    }
}

/// Trait implemented by data guards to derive a value from request body data.
///
/// # Data Guards
///
/// A data guard is a [request guard] that operates on a request's body data.
/// Data guards validate and parse request body data via implementations of `FromData`.
/// In other words, every type that implements `FromData` is a data guard and
/// vice-versa.
///
/// Data guards are the target of the `data` route attribute parameter:
///
/// ```rust
/// # #[macro_use] extern crate rocket;
/// # type DataGuard = rocket::data::Data;
/// #[post("/submit", data = "<var>")]
/// fn submit(var: DataGuard) { /* ... */ }
/// # fn main() { }
/// ```
///
/// A route can have at most one data guard. Above, `var` is used as the
/// argument name for the data guard type `DataGuard`. When the `submit` route
/// matches, Rocket will call the `FromData` implementation for the type `T`.
/// The handler will only be called if the guard returns successfully.
///
/// ## Async Trait
///
/// [`FromData`] is an _async_ trait. Implementations of `FromData` must be
/// decorated with an attribute of `#[rocket::async_trait]`:
///
/// ```rust
/// use rocket::request::Request;
/// use rocket::data::{self, Data, FromData};
/// # struct MyType;
/// # type MyError = String;
///
/// #[rocket::async_trait]
/// impl<'r> FromData<'r> for MyType {
///     type Error = MyError;
///
///     async fn from_data(req: &'r Request<'_>, data: Data) -> data::Outcome<Self, MyError> {
///         /* .. */
///         # unimplemented!()
///     }
/// }
/// ```
///
/// # Example
///
/// Say that you have a custom type, `Person`:
///
/// ```rust
/// struct Person<'r> {
///     name: &'r str,
///     age: u16
/// }
/// ```
///
/// `Person` has a custom serialization format, so the built-in `Json` type
/// doesn't suffice. The format is `<name>:<age>` with `Content-Type:
/// application/x-person`. You'd like to use `Person` as a `FromData` type, or
/// equivalently `FromData`, so that you can retrieve it directly from a
/// client's request body:
///
/// ```rust
/// # use rocket::post;
/// # type Person<'r> = &'r rocket::http::RawStr;
/// #[post("/person", data = "<person>")]
/// fn person(person: Person<'_>) -> &'static str {
///     "Saved the new person to the database!"
/// }
/// ```
///
/// A `FromData` implementation for such a type might look like:
///
/// ```rust
/// # #[macro_use] extern crate rocket;
/// #
/// # #[derive(Debug)]
/// # struct Person<'r> { name: &'r str, age: u16 }
/// #
/// use rocket::request::{self, Request};
/// use rocket::data::{self, Data, FromData, ToByteUnit};
/// use rocket::http::{Status, ContentType};
///
/// enum Error {
///     TooLarge,
///     NoColon,
///     InvalidAge,
///     Io(std::io::Error),
/// }
///
/// #[rocket::async_trait]
/// impl<'r> FromData<'r> for Person<'r> {
///     type Error = Error;
///
///     async fn from_data(req: &'r Request<'_>, data: Data) -> data::Outcome<Self, Error> {
///         use Error::*;
///         use rocket::outcome::Outcome::*;
///
///         // Ensure the content type is correct before opening the data.
///         let person_ct = ContentType::new("application", "x-person");
///         if req.content_type() != Some(&person_ct) {
///             return Forward(data);
///         }
///
///         // Use a configured limit with name 'person' or fallback to default.
///         let limit = req.limits().get("person").unwrap_or(256.bytes());
///
///         // Read the data into a string.
///         let string = match data.open(limit).into_string().await {
///             Ok(string) if string.is_complete() => string.into_inner(),
///             Ok(_) => return Failure((Status::PayloadTooLarge, TooLarge)),
///             Err(e) => return Failure((Status::InternalServerError, Io(e))),
///         };
///
///         // We store the str in request-local cache for long-lived borrows.
///         let string = request::local_cache!(req, string);
///
///         // Split the string into two pieces at ':'.
///         let (name, age) = match string.find(':') {
///             Some(i) => (&string[..i], &string[(i + 1)..]),
///             None => return Failure((Status::UnprocessableEntity, NoColon)),
///         };
///
///         // Parse the age.
///         let age: u16 = match age.parse() {
///             Ok(age) => age,
///             Err(_) => return Failure((Status::UnprocessableEntity, InvalidAge)),
///         };
///
///         Success(Person { name, age })
///     }
/// }
///
/// // The following routes are now possible...
///
/// #[post("/person", data = "<person>")]
/// fn person(person: Person<'_>) { /* .. */ }
///
/// #[post("/person", data = "<person>")]
/// fn person2(person: Result<Person<'_>, Error>) { /* .. */ }
///
/// #[post("/person", data = "<person>")]
/// fn person3(person: Option<Person<'_>>) { /* .. */ }
///
/// #[post("/person", data = "<person>")]
/// fn person4(person: Person<'_>) -> &str {
///     person.name
/// }
///
/// # fn main() {  }
/// ```
#[crate::async_trait]
pub trait FromData<'r>: Sized {
    /// The associated error to be returned when the guard fails.
    type Error: Send;

    /// Asynchronously validates, parses, and converts an instance of `Self`
    /// from the incoming request body data.
    ///
    /// If validation and parsing succeeds, an outcome of `Success` is returned.
    /// If the data is not appropriate given the type of `Self`, `Forward` is
    /// returned. If parsing fails, `Failure` is returned.
    async fn from_data(req: &'r Request<'_>, data: Data) -> Outcome<Self, Self::Error>;
}

use crate::data::Capped;

#[crate::async_trait]
impl<'r> FromData<'r> for Capped<String> {
    type Error = std::io::Error;

    async fn from_data(req: &'r Request<'_>, data: Data) -> Outcome<Self, Self::Error> {
        let limit = req.limits().get("string").unwrap_or(Limits::STRING);
        data.open(limit).into_string().await.into_outcome(Status::BadRequest)
    }
}

impl_strict_from_data_from_capped!(String);

#[crate::async_trait]
impl<'r> FromData<'r> for Capped<&'r RawStr> {
    type Error = std::io::Error;

    async fn from_data(req: &'r Request<'_>, data: Data) -> Outcome<Self, Self::Error> {
        let capped = try_outcome!(<Capped<String>>::from_data(req, data).await);
        let raw = capped.map(|s| RawStr::new(local_cache!(req, s)));
        Success(raw)
    }
}

impl_strict_from_data_from_capped!(&'r RawStr);

#[crate::async_trait]
impl<'r> FromData<'r> for Capped<std::borrow::Cow<'_, str>> {
    type Error = std::io::Error;

    async fn from_data(req: &'r Request<'_>, data: Data) -> Outcome<Self, Self::Error> {
        let capped = try_outcome!(<Capped<String>>::from_data(req, data).await);
        Success(capped.map(|s| s.into()))
    }
}

impl_strict_from_data_from_capped!(std::borrow::Cow<'_, str>);

#[crate::async_trait]
impl<'r> FromData<'r> for Capped<Vec<u8>> {
    type Error = std::io::Error;

    async fn from_data(req: &'r Request<'_>, data: Data) -> Outcome<Self, Self::Error> {
        let limit = req.limits().get("bytes").unwrap_or(Limits::BYTES);
        data.open(limit).into_bytes().await.into_outcome(Status::BadRequest)
    }
}

impl_strict_from_data_from_capped!(Vec<u8>);

#[crate::async_trait]
impl<'r> FromData<'r> for Data {
    type Error = std::convert::Infallible;

    async fn from_data(_: &'r Request<'_>, data: Data) -> Outcome<Self, Self::Error> {
        Success(data)
    }
}

#[crate::async_trait]
impl<'r, T: FromData<'r> + 'r> FromData<'r> for Result<T, T::Error> {
    type Error = std::convert::Infallible;

    async fn from_data(
        req: &'r Request<'_>,
        data: Data
    ) -> Outcome<Result<T, <T as FromData<'r>>::Error>, Self::Error> {
        match T::from_data(req, data).await {
            Success(v) => Success(Ok(v)),
            Failure((_, e)) => Success(Err(e)),
            Forward(d) => Forward(d),
        }
    }
}

#[crate::async_trait]
impl<'r, T: FromData<'r>> FromData<'r> for Option<T> {
    type Error = std::convert::Infallible;

    async fn from_data(req: &'r Request<'_>, data: Data) -> Outcome<Self, Self::Error> {
        match T::from_data(req, data).await {
            Success(v) => Success(Some(v)),
            Failure(..) | Forward(..) => Success(None),
        }
    }
}
