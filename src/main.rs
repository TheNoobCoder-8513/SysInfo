slint::include_modules!();
use std::cell::RefCell;
use std::rc::Rc;
use sysinfo::System;

// This struct will hold our historical data
struct SystemHistory {
    cpu_history: Vec<f32>,
    memory_history: Vec<f32>,
}

const GIB: f32 = 1024.0 * 1024.0 * 1024.0;

impl SystemHistory {
    fn new(size: usize) -> Self {
        Self {
            cpu_history: vec![0.0; size],
            memory_history: vec![0.0; size],
        }
    }

    fn push_cpu(&mut self, val: f32) {
        self.cpu_history.remove(0);
        self.cpu_history.push(val);
    }

    fn push_mem(&mut self, val: f32) {
        self.memory_history.remove(0);
        self.memory_history.push(val);
    }


}

fn main() -> Result<(), slint::PlatformError> {
    let ui = AppWindow::new()?;
    let ui_handle = ui.as_weak();

    let mut sys = System::new_all();

    // Create history state (60 seconds)
    // Wrapped in Rc<RefCell> so it can be moved into the timer closure
    let history = Rc::new(RefCell::new(SystemHistory::new(60)));

    let timer = slint::Timer::default();
    timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(1000),
        move || {
            let ui = match ui_handle.upgrade() {
                Some(ui) => ui,
                None => return,
            };


            sys.refresh_all();

            let mut h = history.borrow_mut();

            // Update history buffers
            let current_cpu = sys.global_cpu_usage();
            let total_mem = sys.total_memory() as f32;
            let used_mem = sys.used_memory() as f32;
            let current_mem_percent = if total_mem > 0.0 {
                (used_mem / total_mem) * 100.0
            } else {
                0.0
            };

            h.push_cpu(current_cpu);
            h.push_mem(current_mem_percent);

            // Update UI with historical data
            ui.set_homeData(gather_home_data(&sys, &h.cpu_history));
            ui.set_cpu_data(gather_cpu_data(&sys, &h.cpu_history));
            ui.set_memory_data(gather_memory_data(&sys, &h.memory_history));
        },
    );

    ui.run()
}

fn gather_home_data(sys: &System, cpu_hist: &[f32]) -> Home_Full_Data {
    let total_mem = sys.total_memory() as f32;
    let used_mem = sys.used_memory() as f32;
    let cpu_usage = sys.global_cpu_usage();
    let cpu_brand = sys
        .cpus()
        .first()
        .map(|c| c.brand())
        .unwrap_or("Unknown CPU");

    Home_Full_Data {
        metric: Home_Metrics_Data {
            cpu: Home_LineGraph_Data {
                lower_val: 0.0,
                upper_val: 100.0,
                curr_val: cpu_usage,
            },
            memory: Home_LineGraph_Data {
                lower_val: 0.0,
                upper_val: (total_mem / GIB * 100.0).round() as f32 / 100.0,
                curr_val: (used_mem / GIB * 100.0).round() as f32 / 100.0,
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
            cpu_name: cpu_brand.into(),
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
        // Now sending the full 60 points to Slint
        chart: Rc::new(slint::VecModel::from(cpu_hist.to_vec())).into(),
    }
}

fn gather_cpu_data(sys: &System, cpu_hist: &[f32]) -> Cpu_Full_Data {
    let global_usage = (sys.global_cpu_usage() * 100.0).round() / 100.0;
    let cpus = sys.cpus();
    let core_usages: Vec<f32> = cpus.iter().map(|cpu| cpu.cpu_usage()).collect();

    let first_cpu = cpus.first();
    let freq = first_cpu.map(|c| c.frequency()).unwrap_or(0);
    let brand = first_cpu.map(|c| c.brand()).unwrap_or("Unknown");
    let load = System::load_average();

    Cpu_Full_Data {
        total_consumption: global_usage,
        cpu_info: Cpu_Info_Data {
            clock_speed: format!("{} MHz", freq).into(),
            core_temp: "N/A".into(),
            avg_Load: format!("{:.2}", load.one).into(),
            freq: format!("{} MHz", freq).into(),
            freq_base: "N/A".into(),
            threads_used: sys.processes().len().to_string().into(),
        },
        processor_info: Processor_Info_Data {
            Model: brand.into(),
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
        cpu_consumption: Rc::new(slint::VecModel::from(cpu_hist.to_vec())).into(),
        core_usage: Rc::new(slint::VecModel::from(core_usages)).into(),
    }
}

fn gather_memory_data(sys: &System, mem_hist: &[f32]) -> Memory_Full_Data {
    let total = sys.total_memory() as f32;
    let used = sys.used_memory() as f32;
    let usage_ratio = if total > 0.0 {
        (used / total) * 100.0
    } else {
        0.0
    };

    Memory_Full_Data {
        total_consumption: (usage_ratio * 100.0).round() / 100.0,
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
        memory_usage: Rc::new(slint::VecModel::from(mem_hist.to_vec())).into(),
    }
}
