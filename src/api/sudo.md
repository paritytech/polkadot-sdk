# Introduction

The functions with the `sudo` prefix are targeted at blockchain node operators who want to inspect the state of their blockchain node.

Contrary to functions with other prefixes, functions with the `sudo` prefix are meant to be called on a specific JSON-RPC server, and not for example on a load balancer. When implementing a load balancer in front of multiple JSON-RPC servers, functions with the `sudo` prefix should be forbidden.
