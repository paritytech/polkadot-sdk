FROM golang:1.14
WORKDIR /opt/relayer
ADD . .
RUN go build -v -o build/snowbridge-relay main.go

FROM parity/subkey:2.0.0
COPY --from=0 /opt/relayer/build/snowbridge-relay /usr/local/bin/
ENTRYPOINT ["/usr/local/bin/snowbridge-relay"]
