slint::include_modules!();
use slint::{ModelRc, StandardListViewItem, VecModel};

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use get_if_addrs::{get_if_addrs, IfAddr};
use sysinfo::{Networks, System};

const GIB: f32 = 1024.0 * 1024.0 * 1024.0;

// ---------------- DATA TYPES ----------------

#[derive(Clone, Default, Copy)]
struct NetworkHistoryPoint {
    upload: f32,
    download: f32,
}

struct SystemHistory {
    cpu_history: Vec<f32>,
    memory_history: Vec<f32>,
    net_history: Vec<NetworkHistoryPoint>,
    last_rx: u64,
    last_tx: u64,
}

impl SystemHistory {
    fn new(size: usize) -> Self {
        Self {
            cpu_history: vec![0.0; size],
            memory_history: vec![0.0; size],
            net_history: vec![NetworkHistoryPoint::default(); size],
            last_rx: 0,
            last_tx: 0,
        }
    }

    fn push_cpu(&mut self, v: f32) {
        self.cpu_history.remove(0);
        self.cpu_history.push(v);
    }

    fn push_mem(&mut self, v: f32) {
        self.memory_history.remove(0);
        self.memory_history.push(v);
    }

    fn push_net(&mut self, up: f32, down: f32) {
        self.net_history.remove(0);
        self.net_history.push(NetworkHistoryPoint {
            upload: up,
            download: down,
        });
    }
}

// ---------------- MAIN ----------------

fn main() -> Result<(), slint::PlatformError> {
    let ui = AppWindow::new()?;
    let ui_handle = ui.as_weak();

    let mut sys = System::new_all();
    let mut networks = Networks::new_with_refreshed_list();
    let history = Rc::new(RefCell::new(SystemHistory::new(60)));

    let timer = slint::Timer::default();
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_secs(1),
        move || {
            let ui = match ui_handle.upgrade() {
                Some(u) => u,
                None => return,
            };

            sys.refresh_cpu_all();
            sys.refresh_memory();
            sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

            networks.refresh(false);

            let mut h = history.borrow_mut();

            // CPU & Memory
            let cpu = sys.global_cpu_usage();
            let mem = (sys.used_memory() as f32 / sys.total_memory() as f32) * 100.0;
            h.push_cpu(cpu);
            h.push_mem(mem);

            ui.set_homeData(gather_home_data(&sys, &h.cpu_history));
            ui.set_cpu_data(gather_cpu_data(&sys, &h.cpu_history));
            ui.set_memory_data(gather_memory_data(&sys, &h.memory_history));

            let is_first_run = h.last_rx == 0 && h.last_tx == 0;

            // Network calculation
            let mut total_rx = 0u64;
            let mut total_tx = 0u64;
            for (_, data) in networks.iter() {
                total_rx += data.total_received();
                total_tx += data.total_transmitted();
            }

            let down = (total_rx.saturating_sub(h.last_rx) as f32 / 1024.0).max(0.0);
            let up = (total_tx.saturating_sub(h.last_tx) as f32 / 1024.0).max(0.0);

            h.last_rx = total_rx;
            h.last_tx = total_tx;

            if !is_first_run {
                h.push_net(up, down);
            }

            ui.set_network_data(gather_network_data(
                &networks,
                &h.net_history,
                total_tx,
                total_rx,
            ));

            let table_data: Vec<Vec<StandardListViewItem>> = gather_process_table_data(&sys);

            // 1. Map rows into ModelRc<StandardListViewItem>
            let row_models: Vec<ModelRc<StandardListViewItem>> = table_data
                .into_iter()
                .map(|row| {
                    // Rc::new(VecModel::from(row)) creates the data
                    // .into() converts it to a ModelRc
                    ModelRc::from(Rc::new(VecModel::from(row)))
                })
                .collect();

            // 2. Wrap the collection of rows into a ModelRc itself
            let final_model = ModelRc::from(Rc::new(VecModel::from(row_models)));

            // 3. Use the name the compiler found (set_process_data)
            ui.set_process_data(final_model);
        },
    );

    ui.run()
}

// ---------------- GATHER FUNCTIONS ----------------

fn gather_home_data(sys: &System, cpu_hist: &[f32]) -> Home_Full_Data {
    let total_mem = sys.total_memory() as f32;
    let used_mem = sys.used_memory() as f32;
    Home_Full_Data {
        metric: Home_Metrics_Data {
            cpu: Home_LineGraph_Data {
                lower_val: 0.0,
                upper_val: 100.0,
                curr_val: sys.global_cpu_usage(),
            },
            memory: Home_LineGraph_Data {
                lower_val: 0.0,
                upper_val: (total_mem / GIB * 100.0).round() / 100.0,
                curr_val: (used_mem / GIB * 100.0).round() / 100.0,
            },
            disk: Home_LineGraph_Data {
                lower_val: 0.0,
                upper_val: 100.0,
                curr_val: 0.0,
            },
            network: Home_LineGraph_Data {
                lower_val: 0.0,
                upper_val: 100.0,
                curr_val: 0.0,
            },
        },
        leftInfoPanel: Home_StaticInfo_Left_Data {
            hostname: System::host_name().unwrap_or_default().into(),
            os: System::name().unwrap_or_default().into(),
            kernel: System::kernel_version().unwrap_or_default().into(),
            cpu_name: sys
                .cpus()
                .first()
                .map(|c| c.brand())
                .unwrap_or("Unknown")
                .into(),
            gpu_model: "N/A".into(),
            total_ram: format!("{:.1} GB", total_mem / 1e9).into(),
            Motherboard: "N/A".into(),
        },
        rightInfoPanel: Home_StaticInfo_Right_Data {
            uptime: format!("{}h", System::uptime() / 3600).into(),
            boot_time: format!("{}", System::boot_time()).into(),
            proc_count: sys.processes().len().to_string().into(),
            local_ip: "127.0.0.1".into(),
        },
        chart: Rc::new(slint::VecModel::from(cpu_hist.to_vec())).into(),
    }
}

fn gather_cpu_data(sys: &System, cpu_hist: &[f32]) -> Cpu_Full_Data {
    let cpus = sys.cpus();
    let core_usages: Vec<f32> = cpus.iter().map(|cpu| cpu.cpu_usage()).collect();
    let y_max = cpu_hist.iter().copied().fold(0.0, f32::max).max(1.0);
    Cpu_Full_Data {
        total_consumption: sys.global_cpu_usage(),
        cpu_info: Cpu_Info_Data {
            clock_speed: format!("{} MHz", cpus.first().map(|c| c.frequency()).unwrap_or(0)).into(),
            core_temp: "N/A".into(),
            avg_Load: format!("{:.2}", System::load_average().one).into(),
            freq: format!("{} MHz", cpus.first().map(|c| c.frequency()).unwrap_or(0)).into(),
            freq_base: "N/A".into(),
            threads_used: sys.processes().len().to_string().into(),
        },
        processor_info: Processor_Info_Data {
            Model: cpus.first().map(|c| c.brand()).unwrap_or("Unknown").into(),
            Cores: System::physical_core_count()
                .unwrap_or(0)
                .to_string()
                .into(),
            Threads: cpus.len().to_string().into(),
            Speed_base: "N/A".into(),
            Architecture: std::env::consts::ARCH.into(),
            Cache_l1: "N/A".into(),
            Cache_l2: "N/A".into(),
            Cache_l3: "N/A".into(),
        },
        cpu_consumption: (
            y_max,
            Rc::new(slint::VecModel::from(cpu_hist.to_vec())).into(),
        ),
        core_usage: Rc::new(slint::VecModel::from(core_usages)).into(),
    }
}

fn gather_memory_data(sys: &System, mem_hist: &[f32]) -> Memory_Full_Data {
    let total = sys.total_memory() as f32;
    let used = sys.used_memory() as f32;
    let y_max = mem_hist.iter().copied().fold(0.0, f32::max).max(1.0);
    Memory_Full_Data {
        total_consumption: if total > 0.0 {
            (used / total) * 100.0
        } else {
            0.0
        },
        physical_info: Physical_Memory_Data {
            installed: format!("{:.1} GB", total / 1e9).into(),
            in_use: format!("{:.1} GB", used / 1e9).into(),
            compressed: "N/A".into(),
            available: format!("{:.1} GB", (total - used) / 1e9).into(),
            reserved: "N/A".into(),
        },
        virtual_info: Virtual_Memory_Data {
            commit_charge: format!("{:.1} GB", sys.used_swap() as f32 / 1e9).into(),
            commit_limit: format!("{:.1} GB", sys.total_swap() as f32 / 1e9).into(),
            commit_percent: "N/A".into(),
            paged_pool: "N/A".into(),
            non_page_pool: "N/A".into(),
        },
        page_info: Page_Info_Data {
            page_faults: "N/A".into(),
            hard_faults: "N/A".into(),
            cache_standby: "N/A".into(),
            modified: "N/A".into(),
        },
        memory_usage: (
            y_max,
            Rc::new(slint::VecModel::from(mem_hist.to_vec())).into(),
        ),
    }
}

fn gather_network_data(
    networks: &Networks,
    hist: &[NetworkHistoryPoint],
    total_tx: u64,
    total_rx: u64,
) -> Network_Full_Data {
    let mut iface_name = "—".to_string();
    let mut mac = "—".to_string();

    for (name, data) in networks.iter() {
        if data.transmitted() > 0 || data.received() > 0 {
            iface_name = name.clone();
            mac = data.mac_address().to_string();
            break;
        }
    }

    let mut ipv4 = "—".to_string();
    let mut ipv6 = "—".to_string();
    if let Ok(ifaces) = get_if_addrs() {
        for iface in ifaces {
            if iface.name == iface_name {
                match iface.addr {
                    IfAddr::V4(v4) => ipv4 = v4.ip.to_string(),
                    IfAddr::V6(v6) => ipv6 = v6.ip.to_string(),
                }
            }
        }
    }

    let last_point = hist.last().cloned().unwrap_or_default();
    let up_vals: Vec<f32> = hist.iter().map(|p| p.upload).collect();
    let down_vals: Vec<f32> = hist.iter().map(|p| p.download).collect();

    let mut interface_list: Vec<Network_Interface_Data> = Vec::new();

    // Get all IP addresses once to avoid repeated system calls
    let all_ips = get_if_addrs().unwrap_or_default();

    for (name, data) in networks.iter() {
        let mut ipv4 = "—".to_string();
        let mut ipv6 = "—".to_string();

        // Find IPs for this specific interface name
        for iface in &all_ips {
            if iface.name == *name {
                match &iface.addr {
                    IfAddr::V4(v4) => ipv4 = v4.ip.to_string(),
                    IfAddr::V6(v6) => ipv6 = v6.ip.to_string(),
                }
            }
        }

        interface_list.push(Network_Interface_Data {
            name: name.clone().into(),
            ipv4: ipv4.into(),
            ipv6: ipv6.into(),
            mac: data.mac_address().to_string().into(),
            // Optional: Show total data per specific interface
            sent: format!("{:.2} MB", data.total_transmitted() as f32 / 1048576.0).into(),
            received: format!("{:.2} MB", data.total_received() as f32 / 1048576.0).into(),
        });
    }

    Network_Full_Data {
        current_speed: Network_Speed_Data {
            upload: last_point.upload,
            download: last_point.download,
        },
        usage: Network_Usage_Data {
            upload: Network_Chart_Data {
                max: up_vals.iter().copied().fold(0.0, f32::max).max(1.0),
                data: Rc::new(slint::VecModel::from(up_vals)).into(),
            },
            download: Network_Chart_Data {
                max: down_vals.iter().copied().fold(0.0, f32::max).max(1.0),
                data: Rc::new(slint::VecModel::from(down_vals)).into(),
            },
        },
        active_info: Network_Active_Info_Data {
            interface_name: iface_name.into(),
            ipv4: ipv4.into(),
            inv6: ipv6.into(),
            mac: mac.into(),
        },
        active_stat: Network_Active_Stat_Data {
            total_sent: (total_tx as f32 * 100.0).round() / 100.0,
            total_received: (total_rx as f32 * 100.0).round() / 100.0,
            interfaces: networks.len() as f32,
            link_status: "Active".into(),
        },
        all_interfaces: Rc::new(slint::VecModel::from(interface_list)).into(),
    }
}

fn gather_process_table_data(sys: &sysinfo::System) -> Vec<Vec<slint::StandardListViewItem>> {
    sys.processes()
        .iter()
        .map(|(pid, proc)| {
            // Each row is a Vec of StandardListViewItem
            vec![
                slint::StandardListViewItem::from(pid.as_u32().to_string().as_str()),
                slint::StandardListViewItem::from(proc.name().to_string_lossy().as_ref()),
                slint::StandardListViewItem::from(format!("{:.1}%", proc.cpu_usage()).as_str()),
                slint::StandardListViewItem::from(
                    format!("{:.1} MB", proc.memory() as f32 / 1024.0 / 1024.0).as_str(),
                ),
            ]
        })
        .collect()
}
