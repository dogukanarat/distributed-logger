FROM rust:latest

WORKDIR /usr/app

COPY . .

RUN apt-get update -y
RUN cargo build

CMD ["cargo", "run"]