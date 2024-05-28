# web_deployment

## build docker image

docker build -t rest-service .

## run image in docker

docker run -d -p 3000:3000 --name rest-service rest-service:latest

## other useful commands

docker run -it --name rest-service rest-service:latest /bin/bash

docker exec -it rest-service /bin/bash
