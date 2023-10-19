FROM ruby:alpine3.13

RUN apk add --no-cache git

ENV APP_HOME /app
ENV RACK_ENV production
RUN mkdir $APP_HOME
WORKDIR $APP_HOME

RUN git clone https://github.com/ananace/ruby-grafana-matrix.git $APP_HOME
RUN bundle install --without development

RUN mkdir /config && touch /config/config.yml && ln -s /config/config.yml ./config.yml

CMD ["bundle", "exec", "rackup", "-p4567"]
