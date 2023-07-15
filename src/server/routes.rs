//! This module provides the functionality to handle different routes of the `websurfx`
//! meta search engine website and provide approriate response to each route/page
//! when requested.

use std::fs::read_to_string;

use crate::{
    cache::cacher::RedisCache,
    config_parser::parser::Config,
    handler::public_path_handler::handle_different_public_path,
    search_results_handler::{aggregation_models::SearchResults, aggregator::aggregate},
};
use actix_web::{get, web, HttpRequest, HttpResponse};
use handlebars::Handlebars;
use serde::Deserialize;

/// A named struct which deserializes all the user provided search parameters and stores them.
///
/// # Fields
///
/// * `q` - It stores the search parameter option `q` (or query in simple words)
/// of the search url.
/// * `page` - It stores the search parameter `page` (or pageno in simple words)
/// of the search url.
#[derive(Deserialize)]
struct SearchParams {
    q: Option<String>,
    page: Option<u32>,
}

/// Handles the route of index page or main page of the `websurfx` meta search engine website.
#[get("/")]
pub async fn index(
    hbs: web::Data<Handlebars<'_>>,
    config: web::Data<Config>,
) -> Result<HttpResponse, Box<dyn std::error::Error>> {
    let page_content: String = hbs.render("index", &config.style).unwrap();
    Ok(HttpResponse::Ok().body(page_content))
}

/// Handles the route of any other accessed route/page which is not provided by the
/// website essentially the 404 error page.
pub async fn not_found(
    hbs: web::Data<Handlebars<'_>>,
    config: web::Data<Config>,
) -> Result<HttpResponse, Box<dyn std::error::Error>> {
    let page_content: String = hbs.render("404", &config.style)?;

    Ok(HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(page_content))
}

/// A named struct which is used to deserialize the cookies fetched from the client side.
///
/// # Fields
///
/// * `theme` - It stores the theme name used in the website.
/// * `colorscheme` - It stores the colorscheme name used for the website theme.
/// * `engines` - It stores the user selected upstream search engines selected from the UI.
#[allow(dead_code)]
#[derive(Deserialize)]
struct Cookie {
    theme: String,
    colorscheme: String,
    engines: Vec<String>,
}

/// Handles the route of search page of the `websurfx` meta search engine website and it takes
/// two search url parameters `q` and `page` where `page` parameter is optional.
///
/// # Example
///
/// ```bash
/// curl "http://127.0.0.1:8080/search?q=sweden&page=1"
/// ```
///
/// Or
///
/// ```bash
/// curl "http://127.0.0.1:8080/search?q=sweden"
/// ```
#[get("/search")]
pub async fn search(
    hbs: web::Data<Handlebars<'_>>,
    req: HttpRequest,
    config: web::Data<Config>,
) -> Result<HttpResponse, Box<dyn std::error::Error>> {
    let params = web::Query::<SearchParams>::from_query(req.query_string())?;

    //Initialize redis cache connection struct
    let mut redis_cache = RedisCache::new(config.redis_connection_url.clone())?;
    match &params.q {
        Some(query) => {
            if query.trim().is_empty() {
                Ok(HttpResponse::Found()
                    .insert_header(("location", "/"))
                    .finish())
            } else {
                let page_url: String; // Declare the page_url variable without initializing it

                // ...

                let page = match params.page {
                    Some(page_number) => {
                        if page_number <= 1 {
                            page_url = format!(
                                "http://{}:{}/search?q={}&page={}",
                                config.binding_ip_addr, config.port, query, 1
                            );
                            1
                        } else {
                            page_url = format!(
                                "http://{}:{}/search?q={}&page={}",
                                config.binding_ip_addr, config.port, query, page_number
                            );

                            page_number
                        }
                    }
                    None => {
                        page_url = format!(
                            "http://{}:{}{}&page={}",
                            config.binding_ip_addr,
                            config.port,
                            req.uri(),
                            1
                        );

                        1
                    }
                };

                // fetch the cached results json.
                let cached_results_json = redis_cache.cached_results_json(&page_url);
                // check if fetched catch results was indeed fetched or it was an error and if so
                // handle the data accordingly.
                match cached_results_json {
                    Ok(results_json) => {
                        let new_results_json: SearchResults = serde_json::from_str(&results_json)?;
                        let page_content: String = hbs.render("search", &new_results_json)?;
                        Ok(HttpResponse::Ok().body(page_content))
                    }
                    Err(_) => {
                        // check if the cookie value is empty or not if it is empty then use the
                        // default selected upstream search engines from the config file otherwise
                        // parse the non-empty cookie and grab the user selected engines from the
                        // UI and use that.
                        let mut results_json: crate::search_results_handler::aggregation_models::SearchResults = match req.cookie("appCookie") {
                            Some(cookie_value) => {
                                    let cookie_value:Cookie = serde_json::from_str(cookie_value.name_value().1)?;
                                    aggregate(query.clone(), page, config.aggregator.random_delay, config.debug, cookie_value.engines).await?
                            },
                            None => aggregate(query.clone(), page, config.aggregator.random_delay, config.debug, config.upstream_search_engines.clone()).await?,
                        };
                        results_json.add_style(config.style.clone());
                        // check whether the results grabbed from the upstream engines are empty or
                        // not if they are empty then set the empty_result_set option to true in
                        // the result json.
                        if results_json.is_empty_result_set() {
                            results_json.set_empty_result_set();
                        }
                        redis_cache
                            .cache_results(serde_json::to_string(&results_json)?, &page_url)?;
                        let page_content: String = hbs.render("search", &results_json)?;
                        Ok(HttpResponse::Ok().body(page_content))
                    }
                }
            }
        }
        None => Ok(HttpResponse::Found()
            .insert_header(("location", "/"))
            .finish()),
    }
}

/// Handles the route of robots.txt page of the `websurfx` meta search engine website.
#[get("/robots.txt")]
pub async fn robots_data(_req: HttpRequest) -> Result<HttpResponse, Box<dyn std::error::Error>> {
    let page_content: String =
        read_to_string(format!("{}/robots.txt", handle_different_public_path()?))?;
    Ok(HttpResponse::Ok()
        .content_type("text/plain; charset=ascii")
        .body(page_content))
}

/// Handles the route of about page of the `websurfx` meta search engine website.
#[get("/about")]
pub async fn about(
    hbs: web::Data<Handlebars<'_>>,
    config: web::Data<Config>,
) -> Result<HttpResponse, Box<dyn std::error::Error>> {
    let page_content: String = hbs.render("about", &config.style)?;
    Ok(HttpResponse::Ok().body(page_content))
}

/// Handles the route of settings page of the `websurfx` meta search engine website.
#[get("/settings")]
pub async fn settings(
    hbs: web::Data<Handlebars<'_>>,
    config: web::Data<Config>,
) -> Result<HttpResponse, Box<dyn std::error::Error>> {
    let page_content: String = hbs.render("settings", &config.style)?;
    Ok(HttpResponse::Ok().body(page_content))
}
