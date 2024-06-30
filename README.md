# fk-zero2prod

Repo for my personal zero2prod code. Zero2prod is an acronym for [Zero To Production In Rust](https://www.zero2prod.com) from [Luca Palmieri](https://github.com/LukeMathWalker). The source code for the book project is [publicly available on GitHub](https://github.com/LukeMathWalker/zero-to-production). Many thanks to Luca Palmieri for this great book.

## deviations from source code for the book

- I use a more centralized approach for error handling, [see](https://github.com/DrFlowerkick/fk-zero2prod/blob/main/src/error.rs). In this approach I replaced the e500() and e400() functions with a conversion from my Error type to actix_web::Error.
- As suggested from the author I turned to [askama](https://docs.rs/askama/latest/askama/) for html rendering for web pages and html and plain text content of emails as an exercise.
- I implemented more or less all exercises suggested by the author.
- In addition I implemented html pages to subscribe and unsubscribe from newsletter. Unsubscribe links are added to all outgpoing emails (emails with confirmation link to subscribe and newsletter emails).
- Finally I'm self hosting my zero2prod app on my [unraid server](https://unraid.net/), using [DuckDNS](https://www.duckdns.org/) and [Nginx Proxy Manager](https://nginxproxymanager.com/) to safely access my app from the internet. Docker building is done via github actions. Since I also self host a PostgresSQL server (seperated in two instances for development and production) I had to create a third Environment type for github to properly run all CI/CD tests.

## about the author

I'm just a hobby programmer playing around with rust. Look out for my first real project [ddc_tournement_planer](https://github.com/DrFlowerkick/ddc_tournement_planer/tree/main).
