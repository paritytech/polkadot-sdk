# Introduction

The functions with the `chainSpec` prefix allow inspecting the content of the specification of the chain a JSON-RPC server is targeting.

Because the chain specification never changes while a JSON-RPC server is running, the return value of all these functions must never change and can be cached by the JSON-RPC client.
