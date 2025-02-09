ngx-fault-injection
===================

A fault injection module for Nginx to simulate server delays and errors for reliability testing.

*This module is not distributed with the Nginx source.* See [Installation](#installation).

Table of Contents
=================

* [Name](#ngx-fault-injection)
* [Status](#status)
	* [Known Bug](#known-bug)
* [Synopsis](#synopsis)
* [Description](#description)
* [Directives](#directives)
	* [fault_injection](#fault_injection)
	* [fault_delay](#fault_delay)
	* [fault_status](#fault_status)
	* [fault_ip](#fault_ip)
* [Client-Controlled Fault Injection](#client-controlled)
* [Acknowledgements](#acknowledgements)
* [Contributing](#contriburing)

Status
======

**Not** ready for production.

Known-bug
---------

Currently this module is using rust `thread::sleep` to wait before sending response. This blocks
nginx worker, making it really slow for multiple requests. The ideal solution is to use the nginx
events api to ensure that it can handle other requests while waiting for this to complete.

**Issue:** [#1](https://github.com/nivaldogmelo/ngx-fault-injection/issues/1)

Synopsis
========

Here's an example configuration that enables fault injection for a specific location, delays
the response by 2000ms, returns a 503 status code and restrictis the fault injection to a
specific IP range:
``` nginx
	location /all {
		# Enables fault injection at this location
		fault_injection on;
		# Defines how much time the server is gonna wait before returning
		fault_delay 2000;
		# Defines which status code the server is gonna answer
		fault_status 503;
		# Defines CIDR that contains the IP list which the fault injection is gonna happen
		fault_ip 255.255.255.255/32;

		proxy_hide_header X-Fault-Injection;

		proxy_pass http://backend;
	}
```

Description
===========

This module allows the usage fault injection within nginx servers. The goal is to use this
to help services to test it's reliability. This can be used to test the scenario where the
server is slow to answer some requests, and see how the client is behaving, to see if it should
enable some circuit breaking, backoff tries or anything else.

This module enables fault injection within Nginx servers, allowing you to simulate various
failure scenarios to test the reliability and resilience of your services. By introducing
artificial delays and errors, you can observe how your system behaves under stress and identify
potential weaknesses.

Key Features:
- **Simulate Delays:** Introduce artificial delays in response to test how your clients handle slow servers.
- **Inject Errors:** Return custom HTTP status codes (e.g, 429, 503) to simulate server failures
- **IP-Based Targeting:** Restrict fault injection to specific IP ranges, enabling controlled testing environments.
- **Client-Controlled Injection:** Allow clients to customize fault injection behavior using the `X-Fault-Injection` header.

Use Cases:
- **Testing Client Resilience:** Simulate slow or failing servers to ensure your clients implement proper retry mechanisms, circuit breakers or fallback strategies.
- **Load Testing:** Evaluate how your system performs under high latency or partial failure conditions.
- **Chaos Engineering:** Proactively test your system's ability to handle unexpected failures in production-like environments.
- **Debugging and Development:** Reproduce specific failure scenarios to debug and improve your application's error-handling logic.
- **Dynamic Testing:** Allow clients to dynamically control fault injection parameters (e.g., delay, status code) for ad-hoc testing.

Directives
==========

The following directives are available through this module.

fault_injection
---------------
**syntax:** *fault_injection on | off;*\
**default:** *fault_injection off;*\
**context:** *location*

Enables or disables fault_injection within the the location.

fault_delay
-----------
**syntax:** *fault_delay \<time\>;*\
**default:** *fault_delay 0s;*\
**context:** *location*

Sets a timer which the server is gonna wait before send the response. The time can be set with the
`s` or `ms` suffixes.

**Example:**

``` nginx
fault_delay 2s; # Wait for 2 seconds before responding
fault_delay 500ms; # Wait for 500 milliseconds before responding
```

fault_status
------------
**syntax:** *fault_status \<code\>;*\
**default:** *--*\
**context:** *location*

Sets the status code to be sent after the delay time has been finished.

fault_ip
--------
**syntax:** *fault_ip \<CIDR\>;*\
**default:** *fault_ip 0.0.0.0/0*\
**context:** *location*

Defines addresses that are allowed to use the fault injection configurations.

Client-Controlled
=================

Clients can customize fault injection behavior by sending the `X-Fault-Injection` header with their
request. The header value can include the following parameters:

- `delay=<time>`: Specifies the delay before the response is sent (e.g., `delay=700ms`).
- `status=<code>`: Specifies the HTTP status code to return (e.g., `status=500`).

**Example Header:**

``` http
X-Fault-Injection: delay=700ms,status=500
```

**Example Configuration:**

``` nginx
location /test {
	fault_injection on;
	fault_delay 1s; # Default delay
	fault_status 503; # Default status code
	fault_ip 192.168.1.0/24; # Restrict to specific IP range

	proxy_pass http://backend;
}
```


Installation
============

This repo provides a Dockerfile that can be used to build the module using a
desired version of the nginx. To do this you can use the following command:

``` bash
docker buildx build -f Dockerfile.build --build-arg NGX_VERSION=<NGX_VERSION> --output=. .
```

This will create the `libngx_fault_injection.so` file that can be used inside your Nginx image.

After building the module, you can load it in your Nginx configuration by adding the following
line to your `nginx.conf`:

``` nginx
load_module /path/to/libngx_fault_injection.so;
```

Acknowledgements
================

- This module was created using [ngx-rust](https://github.com/nginx/ngx-rust)
- This project is inspired by [Istio's Fault injection feature](https://istio.io/latest/docs/tasks/traffic-management/fault-injection/)

Contributing
============

Contributions are welcome! Please feel free to submit a pull request or open an issue to discuss
potential changes or additions.
