use std::{
    collections::HashMap,
    ffi::{c_char, c_void},
    net::Ipv4Addr,
    ptr::addr_of,
    thread,
    time::Duration,
};

use ipnetwork::{IpNetwork, Ipv4Network};
use ngx::{
    core,
    ffi::{nginx_version, ngx_module_t, NGX_HTTP_MODULE, NGX_RS_MODULE_SIGNATURE},
};
use ngx::{
    ffi::{
	ngx_array_push, ngx_command_t, ngx_conf_t, ngx_http_core_module, ngx_http_handler_pt,
	ngx_http_module_t, ngx_http_phases_NGX_HTTP_ACCESS_PHASE, ngx_http_request_t, ngx_int_t,
	ngx_str_t, ngx_uint_t, NGX_CONF_TAKE1, NGX_HTTP_LOC_CONF,
	NGX_HTTP_LOC_CONF_OFFSET,
    },
    http::{self, HTTPModule},
    ngx_null_command, ngx_string,
};


#[derive(Debug)]
struct FaultInjectionConfig {
    delay: Option<Duration>,
    status: Option<u16>,
}

fn parse_fault_injection_header(header_value: &str) -> HashMap<String, String> {
    header_value
	.split(',')
	.map(|param| {
	    let mut parts = param.split('=');
	    let key = parts.next().unwrap().trim().to_string();
	    let value = parts.next().unwrap().trim().to_string();
	    (key, value)
	})
	.collect()
}

fn parse_fault_injection(header_value: &str) -> FaultInjectionConfig {
    let params = parse_fault_injection_header(header_value);

    let delay = params.get("delay").and_then(|v| parse_duration(v));
    let status = params.get("status").and_then(|v| v.parse::<u16>().ok());

    FaultInjectionConfig { delay, status }
}

fn parse_duration(value: &str) -> Option<Duration> {
    if let Some(s) = value.strip_suffix("s") {
	s.parse::<u64>().ok().map(Duration::from_secs)
    } else if let Some(ms) = value.strip_suffix("ms") {
	ms.parse::<u64>().ok().map(Duration::from_millis)
    } else {
	value.parse::<u64>().ok().map(Duration::from_millis)
    }
}

struct FaultInjection;

impl http::HTTPModule for FaultInjection {
    type MainConf = ();
    type SrvConf = ();
    type LocConf = ModuleConfig;

    unsafe extern "C" fn postconfiguration(cf: *mut ngx_conf_t) -> ngx_int_t {
	let cmcf = http::ngx_http_conf_get_module_main_conf(cf, &*addr_of!(ngx_http_core_module));

	let h = ngx_array_push(
	    &mut (*cmcf).phases[ngx_http_phases_NGX_HTTP_ACCESS_PHASE as usize].handlers,
	) as *mut ngx_http_handler_pt;
	if h.is_null() {
	    return core::Status::NGX_ERROR.into();
	}
	// set an Access phase handler
	*h = Some(fault_injection_handler);
	core::Status::NGX_OK.into()
    }
}

#[derive(Debug)]
struct ModuleConfig {
    injection: bool,
    delay: Duration,
    status: u16,
    ip: IpNetwork,
}

impl Default for ModuleConfig {
    fn default() -> Self {
	let ipv4 = Ipv4Addr::new(0, 0, 0, 0);
	let ipv4_prefix: u8 = 0;
	let ipv4 = Ipv4Network::new(ipv4, ipv4_prefix).expect("Invalid CIDR range");

	Self {
	    injection: false,
	    delay: Duration::from_secs(0),
	    status: 0,
	    ip: IpNetwork::V4(ipv4),
	}
    }
}

impl http::Merge for ModuleConfig {
    fn merge(&mut self, prev: &Self) -> Result<(), http::MergeConfigError> {
	if prev.injection {
	    self.injection = true;
	}

	if self.delay.is_zero() {
	    if !prev.delay.is_zero() {
		self.delay = prev.delay;
	    } else {
		self.delay = Duration::from_secs(0);
	    }
	}

	if self.status == 0 {
	    if !prev.status == 0 {
		self.status = prev.status;
	    }
	}

	let ipv4 = Ipv4Addr::new(0, 0, 0, 0);
	let ipv4_prefix: u8 = 0;
	let ipv4 = Ipv4Network::new(ipv4, ipv4_prefix).expect("Invalid CIDR range");
	let default_ip = IpNetwork::V4(ipv4);

	if self.ip.eq(&default_ip) {
	    self.ip = prev.ip;
	}

	Ok(())
    }
}

static mut NGX_HTTP_FAULT_INJECTION_COMMANDS: [ngx_command_t; 5] = [
    ngx_command_t {
	name: ngx_string!("fault_injection"),
	type_: (NGX_HTTP_LOC_CONF | NGX_CONF_TAKE1) as ngx_uint_t,
	set: Some(ngx_http_fault_injection_commands_set),
	conf: NGX_HTTP_LOC_CONF_OFFSET,
	offset: 0,
	post: std::ptr::null_mut(),
    },
    ngx_command_t {
	name: ngx_string!("fault_delay"),
	type_: (NGX_HTTP_LOC_CONF | NGX_CONF_TAKE1) as ngx_uint_t,
	set: Some(ngx_http_fault_injection_commands_set),
	conf: NGX_HTTP_LOC_CONF_OFFSET,
	offset: 0,
	post: std::ptr::null_mut(),
    },
    ngx_command_t {
	name: ngx_string!("fault_status"),
	type_: (NGX_HTTP_LOC_CONF | NGX_CONF_TAKE1) as ngx_uint_t,
	set: Some(ngx_http_fault_injection_commands_set),
	conf: NGX_HTTP_LOC_CONF_OFFSET,
	offset: 0,
	post: std::ptr::null_mut(),
    },
    ngx_command_t {
	name: ngx_string!("fault_ip"),
	type_: (NGX_HTTP_LOC_CONF | NGX_CONF_TAKE1) as ngx_uint_t,
	set: Some(ngx_http_fault_injection_commands_set),
	conf: NGX_HTTP_LOC_CONF_OFFSET,
	offset: 0,
	post: std::ptr::null_mut(),
    },
    ngx_null_command!(),
];

extern "C" fn ngx_http_fault_injection_commands_set(
    cf: *mut ngx_conf_t,
    _cmd: *mut ngx_command_t,
    conf: *mut c_void,
) -> *mut c_char {
    unsafe {
	let conf = &mut *(conf as *mut ModuleConfig);
	let args = (*(*cf).args).elts as *mut ngx_str_t;

	let directive = (*args.add(0)).to_str();
	let val = (*args.add(1)).to_str();

	match directive {
	    "fault_injection" => {
		if val.len() == 2 && val.eq_ignore_ascii_case("on") {
		    conf.injection = true;
		}
	    }
	    "fault_delay" => {
		// TODO: Accept different time measures: ms, s ////////////////
		let delay = val.to_string();
		println!("{:?}", delay);
		let delay = parse_duration(&delay).unwrap();
		// let delay = Duration::from_millis(delay.parse().unwrap());
		conf.delay = delay;
	    }
	    "fault_status" => {
		let status = val.to_string();
		let status = status.parse().unwrap();
		conf.status = status;
	    }
	    "fault_ip" => {
		let ipv4: Ipv4Network = val.parse().expect("Wrong IPv4 format");
		let ip = IpNetwork::V4(ipv4);
		conf.ip = ip;
	    }
	    _ => (),
	};
    };

    std::ptr::null_mut()
}

static NGX_HTTP_FAULT_INJECTION_MODULE_CTX: ngx_http_module_t = ngx_http_module_t {
    preconfiguration: Some(FaultInjection::preconfiguration),
    postconfiguration: Some(FaultInjection::postconfiguration),
    create_main_conf: Some(FaultInjection::create_main_conf),
    init_main_conf: Some(FaultInjection::init_main_conf),
    create_srv_conf: Some(FaultInjection::create_srv_conf),
    merge_srv_conf: Some(FaultInjection::merge_srv_conf),
    create_loc_conf: Some(FaultInjection::create_loc_conf),
    merge_loc_conf: Some(FaultInjection::merge_loc_conf),
};

extern "C" fn fault_injection_handler(r: *mut ngx_http_request_t) -> ngx_int_t {
    let real_ip = match unsafe { (*r).headers_in.x_real_ip.as_mut() } {
	Some(kv) => kv.value.to_str(),
	None => "",
    };
    let status: core::Status =
	request_handler(unsafe { http::Request::from_ngx_http_request(r) }, real_ip);
    status.0
}

fn request_handler(request: &mut http::Request, real_ip: &str) -> core::Status {
    let co = unsafe {
	request.get_module_loc_conf::<ModuleConfig>(&*addr_of!(ngx_http_fault_injection_module))
    };
    let co = co.expect("FaultInjection module is not found");

    match co.injection {
	true => {
	    let mut client_ip = "";
	    if real_ip.is_empty() {
		client_ip = unsafe { (*request.connection()).addr_text.to_str() };
	    } else {
		client_ip = real_ip;
	    }

	    if co.ip.contains(client_ip.parse().unwrap()) {
		let mut sleep_time = co.delay;
		let mut status = co.status;

		for (name, value) in request.headers_in_iterator() {
		    if name.eq_ignore_ascii_case("X-Fault-Injection") {
			let fault_config = parse_fault_injection(value);

			if fault_config.delay.is_some() {
			    sleep_time = fault_config.delay.unwrap();
			}

			if fault_config.status.is_some() {
			    status = fault_config.status.unwrap();
			}

			break;
		    }
		}

		// Use tokio to not block nginx worker ////////////////////////
		thread::sleep(sleep_time);

		match http::HTTPStatus::from_u16(status) {
		    Ok(s) => s.into(),
		    Err(_) => core::Status::NGX_DECLINED,
		}
	    } else {
		core::Status::NGX_DECLINED
	    }
	}
	false => core::Status::NGX_DECLINED,
    }
}

// Generate the `ngx_modules` table with exported modules.
// This feature is required to build a 'cdylib' dynamic module outside of the NGINX buildsystem.
#[cfg(feature = "export-modules")]
ngx::ngx_modules!(ngx_http_fault_injection_module);

#[used]
#[allow(non_upper_case_globals)]
#[cfg_attr(not(feature = "export-modules"), no_mangle)]
pub static mut ngx_http_fault_injection_module: ngx_module_t = ngx_module_t {
    ctx_index: ngx_uint_t::MAX,
    index: ngx_uint_t::MAX,
    name: std::ptr::null_mut(),
    spare0: 0,
    spare1: 0,
    version: nginx_version as ngx_uint_t,
    signature: NGX_RS_MODULE_SIGNATURE.as_ptr() as *const c_char,

    ctx: &NGX_HTTP_FAULT_INJECTION_MODULE_CTX as *const _ as *mut _,
    commands: unsafe { &NGX_HTTP_FAULT_INJECTION_COMMANDS[0] as *const _ as *mut _ },
    type_: NGX_HTTP_MODULE as ngx_uint_t,

    init_master: None,
    init_module: None,
    init_process: None,
    init_thread: None,
    exit_thread: None,
    exit_process: None,
    exit_master: None,

    spare_hook0: 0,
    spare_hook1: 0,
    spare_hook2: 0,
    spare_hook3: 0,
    spare_hook4: 0,
    spare_hook5: 0,
    spare_hook6: 0,
    spare_hook7: 0,
};
