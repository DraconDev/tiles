  1770	fn handle_event(evt: Event, app: &mut App, event_tx: mpsc::Sender<AppEvent>) -> bool {
  1771	    match evt {
  1772	        Event::Resize(w, h) => {
  1773	            if let Some(until) = app.ignore_resize_until {
  1774	                if std::time::Instant::now() < until {
  1775	                    return true;
  1776	                }
  1777	            }
  1778	            app.terminal_size = (w, h);
  1779	            return true;
  1780	        }
  1781	        Event::Key(key) => {
  1782	            crate::app::log_debug(&format!("KEY EVENT: code={:?} modifiers={:?}", key.code, key.modifiers));
  1783	            
  1784	            // 1. Global Shortcuts (Highest Priority)
  1785	            let has_control = key.modifiers.contains(KeyModifiers::CONTROL);
  1786	
  1787	            match key.code {
  1788	                KeyCode::Char('q') | KeyCode::Char('Q') if has_control => { app.running = false; return true; }
  1789	                KeyCode::Char('b') | KeyCode::Char('B') if has_control => { app.show_sidebar = !app.show_sidebar; return true; }
  1790	                KeyCode::Char('i') | KeyCode::Char('I') if has_control => {
  1791	                    let state = crate::modules::introspection::WorldState::capture(app);
  1792	                    if let Ok(json) = serde_json::to_string_pretty(&state) {
  1793	                        let _ = std::fs::write("introspection.json", json);
  1794	                        app.last_action_msg = Some(("World state dumped to introspection.json".to_string(), std::time::Instant::now()));
  1795	                    }
  1796	                    return true;
  1797	                }
  1798	                KeyCode::Char('s') | KeyCode::Char('S') if has_control => { app.toggle_split(); let _ = event_tx.try_send(AppEvent::RefreshFiles(0)); let _ = event_tx.try_send(AppEvent::RefreshFiles(1)); return true; }
  1799	                KeyCode::Char('\\') if has_control => { app.toggle_split(); let _ = event_tx.try_send(AppEvent::RefreshFiles(0)); let _ = event_tx.try_send(AppEvent::RefreshFiles(1)); return true; }
  1800	                KeyCode::Char('h') | KeyCode::Char('H') if has_control => { let idx = app.toggle_hidden(); let _ = event_tx.try_send(AppEvent::RefreshFiles(idx)); return true; }
  1801	                KeyCode::Char('g') | KeyCode::Char('G') if has_control => { app.mode = AppMode::Settings; return true; }
  1802	                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Char('o') | KeyCode::Char('O') if has_control => {
  1803	                    if let Some(pane) = app.panes.get(app.focused_pane_index) {
  1804	                        if let Some(fs) = pane.current_state() {
  1805	                            let _ = event_tx.try_send(AppEvent::SpawnTerminal {
  1806	                                path: fs.current_path.clone(),
  1807	                                new_tab: true, // Use 'true' (--tab) as it reliably opens a window on this system
  1808	                                remote: fs.remote_session.clone(),
  1809	                                command: None,
  1810	                            });
  1811	                        }
  1812	                    }
  1813	                    return true;
  1814	                }
  1815	                KeyCode::Char('t') | KeyCode::Char('T') if has_control => {
  1816	                    if let Some(pane) = app.panes.get_mut(app.focused_pane_index) {
  1817	                        if let Some(fs) = pane.current_state() {
  1818	                            let new_fs = fs.clone(); // Clone state exactly, preserving selection
  1819	                            pane.open_tab(new_fs);
  1820	                            let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
  1821	                        }
  1822	                    }
  1823	                    return true;
  1824	                }
  1825	                KeyCode::Char(' ') if !has_control => {
  1826	                    if let Some(fs) = app.current_file_state() {
  1827	                        if let Some(idx) = fs.selected_index {
  1828	                            if let Some(path) = fs.files.get(idx) {
  1829	                                if path.is_dir() {
  1830	                                    let target = path.clone();
  1831	                                    let tx = event_tx.clone();
  1832	                                    
  1833	                                    app.last_action_msg = Some((format!("Calculating size: {}...", target.file_name().unwrap_or_default().to_string_lossy()), std::time::Instant::now()));
  1834	                                    
  1835	                                    tokio::spawn(async move {
  1836	                                        let mut total_size = 0;
  1837	                                        let mut stack = vec![target.clone()];
  1838	                                        while let Some(p) = stack.pop() {
  1839	                                            if let Ok(entries) = std::fs::read_dir(p) {
  1840	                                                for entry in entries.flatten() {
  1841	                                                    if let Ok(meta) = entry.metadata() {
  1842	                                                        if meta.is_dir() { stack.push(entry.path()); }
  1843	                                                        else { total_size += meta.len(); }
  1844	                                                    }
  1845	                                                }
  1846	                                            }
  1847	                                        }
  1848	                                        
  1849	                                        let size_str = if total_size < 1024 { format!("{} B", total_size) }
  1850	                                                       else if total_size < 1024 * 1024 { format!("{:.1} KB", total_size as f64 / 1024.0) }
  1851	                                                       else if total_size < 1024 * 1024 * 1024 { format!("{:.1} MB", total_size as f64 / 1024.0 / 1024.0) }
  1852	                                                       else { format!("{:.1} GB", total_size as f64 / 1024.0 / 1024.0 / 1024.0) };
  1853	
  1854	                                        let _ = tx.send(AppEvent::StatusMsg(format!("Size of {}: {}", target.file_name().unwrap_or_default().to_string_lossy(), size_str))).await;
  1855	                                    });
  1856	                                }
  1857	                            }
  1858	                        }
  1859	                    }
  1860	                    return true;
  1861	                }
  1862	                KeyCode::Char(' ') if has_control => { 
  1863	                    app.input.clear(); 
  1864	                    app.mode = AppMode::CommandPalette; 
  1865	                    update_commands(app); 
  1866	                    return true; 
  1867	                }
  1868	                KeyCode::Left if has_control => {
  1869	                    if app.sidebar_focus {
  1870	                        app.resize_sidebar(-2);
  1871	                    } else {
  1872	                        app.move_to_other_pane(); 
  1873	                        let _ = event_tx.try_send(AppEvent::RefreshFiles(0)); 
  1874	                        let _ = event_tx.try_send(AppEvent::RefreshFiles(1)); 
  1875	                    }
  1876	                    return true;
  1877	                }
  1878	                KeyCode::Right if has_control => {
  1879	                    if app.sidebar_focus {
  1880	                        app.resize_sidebar(2);
  1881	                    } else {
  1882	                        app.move_to_other_pane(); 
  1883	                        let _ = event_tx.try_send(AppEvent::RefreshFiles(0)); 
  1884	                        let _ = event_tx.try_send(AppEvent::RefreshFiles(1)); 
  1885	                    }
  1886	                    return true;
  1887	                }
  1888	                _ => {}
  1889	            }
  1890	
  1891	            match &app.mode {
  1892	                AppMode::CommandPalette => {
  1893	                    match key.code {
  1894	                        KeyCode::Esc => { app.mode = AppMode::Normal; return true; }
  1895	                        KeyCode::Enter => { 
  1896	                            if let Some(cmd) = app.filtered_commands.get(app.command_index).cloned() { 
  1897	                                execute_command(cmd.action, app, event_tx.clone()); 
  1898	                            } 
  1899	                            app.mode = AppMode::Normal; 
  1900	                            app.input.clear();
  1901	                            return true;
  1902	                        }
  1903	                        _ => {
  1904	                            let handled = app.input.handle_event(&evt);
  1905	                            if handled { update_commands(app); }
  1906	                            return handled;
  1907	                        }
  1908	                    }
  1909	                }
  1910	                AppMode::AddRemote(idx) => {
  1911	                    let idx = *idx;
  1912	                    match key.code {
  1913	                        KeyCode::Esc => { app.mode = AppMode::Normal; app.input.clear(); return true; }
  1914	                        KeyCode::Tab | KeyCode::Enter => {
  1915	                            let val = app.input.value.clone();
  1916	                            match idx {
  1917	                                0 => app.pending_remote.name = val,
  1918	                                1 => app.pending_remote.host = val,
  1919	                                2 => app.pending_remote.user = val,
  1920	                                3 => app.pending_remote.port = val.parse().unwrap_or(22),
  1921	                                4 => app.pending_remote.key_path = if val.is_empty() { None } else { Some(std::path::PathBuf::from(val)) },
  1922	                                _ => {}
  1923	                            }
  1924	                            if idx < 4 {
  1925	                                app.mode = AppMode::AddRemote(idx + 1);
  1926	                                let next_val = match idx + 1 {
  1927	                                    1 => app.pending_remote.host.clone(),
  1928	                                    2 => app.pending_remote.user.clone(),
  1929	                                    3 => app.pending_remote.port.to_string(),
  1930	                                    4 => app.pending_remote.key_path.as_ref().map(|p| p.to_string_lossy().to_string()).unwrap_or_default(),
  1931	                                    _ => String::new(),
  1932	                                };
  1933	                                app.input.set_value(next_val);
  1934	                            } else {
  1935	                                app.remote_bookmarks.push(app.pending_remote.clone());
  1936	                                let _ = crate::config::save_state(app);
  1937	                                app.mode = AppMode::Normal;
  1938	                                app.input.clear();
  1939	                            }
  1940	                            return true;
  1941	                        }
  1942	                        _ => { return app.input.handle_event(&evt); }
  1943	                    }
  1944	                }
  1945	                AppMode::Header(idx) => {
  1946	                    let idx = *idx;
  1947	                    let total_icons = 5;
  1948	                    let total_tabs: usize = app.panes.iter().map(|p| p.tabs.len()).sum();
  1949	                    let total_items = total_icons + total_tabs;
  1950	                    match key.code {
  1951	                        KeyCode::Esc => { app.mode = AppMode::Normal; return true; }
  1952	                        KeyCode::Left => { app.mode = AppMode::Header(idx.saturating_sub(1)); return true; }
  1953	                        KeyCode::Right => { if idx < total_items.saturating_sub(1) { app.mode = AppMode::Header(idx + 1); } return true; }
  1954	                        KeyCode::Down => { app.mode = AppMode::Normal; return true; }
  1955	                        KeyCode::Enter => {
  1956	                            if idx < total_icons {
  1957	                                let action_id = match idx { 0 => "burger", 1 => "back", 2 => "forward", 3 => "split", 4 => "reset", _ => "" };
  1958	                                match action_id {
  1959	                                    "burger" => app.mode = AppMode::Settings,
  1960	                                    "back" => if let Some(fs) = app.current_file_state_mut() { navigate_back(fs); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); }
  1961	                                    "forward" => if let Some(fs) = app.current_file_state_mut() { navigate_forward(fs); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); }
  1962	                                    "split" => { app.toggle_split(); let _ = event_tx.try_send(AppEvent::RefreshFiles(0)); let _ = event_tx.try_send(AppEvent::RefreshFiles(1)); }
  1963	                                    "reset" => app.mode = AppMode::ConfirmReset,
  1964	                                    _ => {}
  1965	                                }
  1966	                                if let AppMode::Header(_) = app.mode { app.mode = AppMode::Normal; }
  1967	                            } else {
  1968	                                let mut current_global_tab = 5;
  1969	                                for (p_i, pane) in app.panes.iter_mut().enumerate() {
  1970	                                    let mut found = false;
  1971	                                    for (t_i, _) in pane.tabs.iter().enumerate() {
  1972	                                        if current_global_tab == idx {
  1973	                                            pane.active_tab_index = t_i;
  1974	                                            app.focused_pane_index = p_i;
  1975	                                            let _ = event_tx.try_send(AppEvent::RefreshFiles(p_i));
  1976	                                            app.mode = AppMode::Normal;
  1977	                                            found = true;
  1978	                                            break;
  1979	                                        }
  1980	                                        current_global_tab += 1;
  1981	                                    }
  1982	                                    if found { break; }
  1983	                                }
  1984	                            }
  1985	                            return true;
  1986	                        }
  1987	                        _ => {}
  1988	                    }
  1989	                    return true;
  1990	                }
  1991	                AppMode::OpenWith(path) => {
  1992	                    match key.code {
  1993	                        KeyCode::Esc => { app.mode = AppMode::Normal; app.input.clear(); return true; }
  1994	                        KeyCode::Enter => {
  1995	                            let cmd = app.input.value.clone();
  1996	                            if !cmd.is_empty() { let _ = event_tx.try_send(AppEvent::SpawnDetached { cmd, args: vec![path.to_string_lossy().to_string()] }); }
  1997	                            app.mode = AppMode::Normal; app.input.clear();
  1998	                            return true;
  1999	                        }
  2000	                        _ => { return app.input.handle_event(&evt); }
  2001	                    }
  2002	                }
  2003	                AppMode::ConfirmReset => {
  2004	                    crate::app::log_debug("ConfirmReset mode active, waiting for input...");
  2005	                    match key.code {
  2006	                        KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
  2007	                            crate::app::log_debug("Reset confirmed via keyboard");
  2008	                            for (i, pane) in app.panes.iter_mut().enumerate() {
  2009	                                if let Some(fs) = pane.current_state_mut() {
  2010	                                    fs.column_widths.insert(crate::app::FileColumn::Name, 30);
  2011	                                    fs.column_widths.insert(crate::app::FileColumn::Size, 10);
  2012	                                    fs.column_widths.insert(crate::app::FileColumn::Modified, 20);
  2013	                                    fs.column_widths.insert(crate::app::FileColumn::Permissions, 12);
  2014	                                    *fs.table_state.offset_mut() = 0;
  2015	                                    let _ = event_tx.try_send(AppEvent::RefreshFiles(i));
  2016	                                }
  2017	                            }
  2018	                            let _ = crate::config::save_state(app);
  2019	                            let _ = event_tx.try_send(AppEvent::StatusMsg("All column widths reset to defaults".to_string()));
  2020	                            app.mode = AppMode::Normal;
  2021	                            return true;
  2022	                        }
  2023	                        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => { 
  2024	                            crate::app::log_debug("Reset cancelled via keyboard");
  2025	                            app.mode = AppMode::Normal; 
  2026	                            return true; 
  2027	                        }
  2028	                        _ => {}
  2029	                    }
  2030	                    return true;
  2031	                }
  2032	                AppMode::Highlight => {
  2033	                    if let KeyCode::Char(c) = key.code {
  2034	                        if let Some(digit) = c.to_digit(10) {
  2035	                            if digit <= 6 {
  2036	                                let color = if digit == 0 { None } else { Some(digit as u8) };
  2037	                                if let Some(fs) = app.current_file_state() {
  2038	                                    let mut paths = Vec::new();
  2039	                                    if !fs.multi_select.is_empty() {
  2040	                                        for &idx in &fs.multi_select { if let Some(p) = fs.files.get(idx) { paths.push(p.clone()); } }
  2041	                                    } else if let Some(idx) = fs.selected_index { if let Some(p) = fs.files.get(idx) { paths.push(p.clone()); } }
  2042	                                    for p in paths { if let Some(col) = color { app.path_colors.insert(p, col); } else { app.path_colors.remove(&p); } }
  2043	                                    let _ = crate::config::save_state(app);
  2044	                                }
  2045	                                app.mode = AppMode::Normal; return true;
  2046	                            }
  2047	                        }
  2048	                    } else if key.code == KeyCode::Esc { app.mode = AppMode::Normal; return true; }
  2049	                    return false;
  2050	                }
  2051	                AppMode::Settings => {
  2052	                    match key.code {
  2053	                        KeyCode::Esc => { app.mode = AppMode::Normal; return true; }
  2054	                        KeyCode::Char('1') => { app.settings_target = SettingsTarget::SingleMode; return true; }
  2055	                        KeyCode::Char('2') => { app.settings_target = SettingsTarget::SplitMode; return true; }
  2056	                        KeyCode::Left | KeyCode::BackTab => { app.settings_section = match app.settings_section { SettingsSection::Columns => SettingsSection::Shortcuts, SettingsSection::Tabs => SettingsSection::Columns, SettingsSection::General => SettingsSection::Tabs, SettingsSection::Remotes => SettingsSection::General, SettingsSection::Shortcuts => SettingsSection::Remotes }; return true; } 
  2057	                        KeyCode::Right | KeyCode::Tab => { app.settings_section = match app.settings_section { SettingsSection::Columns => SettingsSection::Tabs, SettingsSection::Tabs => SettingsSection::General, SettingsSection::General => SettingsSection::Remotes, SettingsSection::Remotes => SettingsSection::Shortcuts, SettingsSection::Shortcuts => SettingsSection::Columns }; return true; } 
  2058	                        KeyCode::Char('n') => { app.toggle_column(crate::app::FileColumn::Name); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); return true; } 
  2059	                        KeyCode::Char('e') => { app.toggle_column(crate::app::FileColumn::Extension); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); return true; } 
  2060	                        KeyCode::Char('s') => { app.toggle_column(crate::app::FileColumn::Size); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); return true; } 
  2061	                        KeyCode::Char('m') => { app.toggle_column(crate::app::FileColumn::Modified); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); return true; } 
  2062	                        KeyCode::Char('c') => { app.toggle_column(crate::app::FileColumn::Created); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); return true; } 
  2063	                        KeyCode::Char('p') => { app.toggle_column(crate::app::FileColumn::Permissions); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); return true; } 
  2064	                        KeyCode::Char('i') => {
  2065	                            app.icon_mode = match app.icon_mode { IconMode::Nerd => IconMode::Unicode, IconMode::Unicode => IconMode::ASCII, IconMode::ASCII => IconMode::Nerd };
  2066	                            return true;
  2067	                        }
  2068	                        KeyCode::Char('h') if app.settings_section == SettingsSection::General => { app.default_show_hidden = !app.default_show_hidden; return true; } 
  2069	                        KeyCode::Char('d') if app.settings_section == SettingsSection::General => { app.confirm_delete = !app.confirm_delete; return true; } 
  2070	                        _ => { return false; } 
  2071	                    }
  2072	                }
  2073	                AppMode::ImportServers => {
  2074	                    match key.code {
  2075	                        KeyCode::Esc => { app.mode = AppMode::Normal; return true; }
  2076	                        KeyCode::Enter => {
  2077	                            let filename = app.input.value.clone();
  2078	                            let import_path = if let Some(fs) = app.current_file_state() { fs.current_path.join(filename) } else { std::path::PathBuf::from(filename) };
  2079	                            let _ = app.import_servers(import_path); let _ = crate::config::save_state(app);
  2080	                            app.mode = AppMode::Normal; app.input.clear(); return true;
  2081	                        }
  2082	                        _ => { return app.input.handle_event(&evt); }
  2083	                    }
  2084	                }
  2085	                AppMode::NewFile | AppMode::NewFolder | AppMode::Rename | AppMode::Delete => {
  2086	                    if app.mode == AppMode::Rename && app.rename_selected {
  2087	                        match key.code {
  2088	                            KeyCode::Char(c) => {
  2089	                                app.rename_selected = false;
  2090	                                let input_val = app.input.value.clone();
  2091	                                let path = std::path::Path::new(&input_val);
  2092	                                if let Some(stem) = path.file_stem() {
  2093	                                    if let Some(ext) = path.extension() {
  2094	                                        if !stem.to_string_lossy().is_empty() { app.input.set_value(format!("{}.{}", c, ext.to_string_lossy())); } 
  2095	                                        else { app.input.set_value(c.to_string()); }
  2096	                                    } else { app.input.set_value(c.to_string()); }
  2097	                                } else { app.input.set_value(c.to_string()); }
  2098	                                return true;
  2099	                            }
  2100	                            KeyCode::Backspace => {
  2101	                                app.rename_selected = false;
  2102	                                let input_val = app.input.value.clone();
  2103	                                let path = std::path::Path::new(&input_val);
  2104	                                if let Some(ext) = path.extension() { app.input.set_value(format!(".{}", ext.to_string_lossy())); } 
  2105	                                else { app.input.clear(); }
  2106	                                return true;
  2107	                            }
  2108	                            KeyCode::Left | KeyCode::Right => { app.rename_selected = false; }
  2109	                            KeyCode::Esc => { app.mode = AppMode::Normal; app.input.clear(); return true; }
  2110	                            _ => {}
  2111	                        }
  2112	                    }
  2113	                    match key.code {
  2114	                        KeyCode::Esc => { app.mode = AppMode::Normal; app.input.clear(); return true; }
  2115	                        KeyCode::Enter => {
  2116	                            let input = app.input.value.clone();
  2117	                            if let Some(fs) = app.current_file_state() {
  2118	                                let path = fs.current_path.join(&input);
  2119	                                match app.mode {
  2120	                                    AppMode::NewFile => { let _ = event_tx.try_send(AppEvent::CreateFile(path)); }
  2121	                                    AppMode::NewFolder => { let _ = event_tx.try_send(AppEvent::CreateFolder(path)); }
  2122	                                    AppMode::Rename => {
  2123	                                        if let Some(idx) = fs.selected_index {
  2124	                                            if let Some(old_path) = fs.files.get(idx) {
  2125	                                                let new_path = old_path.parent().unwrap_or(&std::path::PathBuf::from(".")).join(&input);
  2126	                                                let _ = event_tx.try_send(AppEvent::Rename(old_path.clone(), new_path));
  2127	                                            }
  2128	                                        }
  2129	                                    }
  2130	                                    AppMode::Delete => {
  2131	                                        let input_clean = input.trim().to_lowercase();
  2132	                                        if input_clean == "y" || input_clean == "yes" || input_clean.is_empty() || !app.confirm_delete {
  2133	                                            let mut paths_to_delete = Vec::new();
  2134	                                            if !fs.multi_select.is_empty() { for &idx in &fs.multi_select { if let Some(p) = fs.files.get(idx) { paths_to_delete.push(p.clone()); } } } 
  2135	                                            else if let Some(idx) = fs.selected_index { if let Some(path) = fs.files.get(idx) { paths_to_delete.push(path.clone()); } }
  2136	                                            for p in paths_to_delete { let _ = event_tx.try_send(AppEvent::Delete(p)); }
  2137	                                        }
  2138	                                    }
  2139	                                    _ => {} 
  2140	                                }
  2141	                            }
  2142	                            app.mode = AppMode::Normal; app.input.clear(); return true;
  2143	                        }
  2144	                        _ => { return app.input.handle_event(&evt); }
  2145	                    }
  2146	                }
  2147	                _ => {
  2148	                    if key.code == KeyCode::Esc {
  2149	                        app.mode = AppMode::Normal;
  2150	                        for pane in &mut app.panes { pane.preview = None; }
  2151	                        if let Some(fs) = app.current_file_state_mut() { 
  2152	                            fs.multi_select.clear(); 
  2153	                            fs.selection_anchor = None; 
  2154	                            if !fs.search_filter.is_empty() { 
  2155	                                fs.search_filter.clear(); fs.selected_index = Some(0); *fs.table_state.offset_mut() = 0; 
  2156	                                let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); 
  2157	                            } 
  2158	                        } 
  2159	                        return true;
  2160	                    }
  2161	                    match key.code {
  2162	                        KeyCode::Char('c') if has_control => {
  2163	                            if let Some(fs) = app.current_file_state() {
  2164	                                if let Some(idx) = fs.selected_index {
  2165	                                    if let Some(path) = fs.files.get(idx) {
  2166	                                        app.clipboard = Some((path.clone(), crate::app::ClipboardOp::Copy));
  2167	                                    }
  2168	                                }
  2169	                            }
  2170	                            return true;
  2171	                        }
  2172	                        KeyCode::Char('x') if has_control => {
  2173	                            if let Some(fs) = app.current_file_state() {
  2174	                                if let Some(idx) = fs.selected_index {
  2175	                                    if let Some(path) = fs.files.get(idx) {
  2176	                                        app.clipboard = Some((path.clone(), crate::app::ClipboardOp::Cut));
  2177	                                    }
  2178	                                }
  2179	                            }
  2180	                            return true;
  2181	                        }
  2182	                        KeyCode::Char('v') if has_control => {
  2183	                            if let Some((src, op)) = app.clipboard.clone() {
  2184	                                if let Some(fs) = app.current_file_state() {
  2185	                                    let dest = fs.current_path.join(src.file_name().unwrap());
  2186	                                    match op {
  2187	                                        crate::app::ClipboardOp::Copy => { let _ = event_tx.try_send(AppEvent::Copy(src, dest)); }
  2188	                                        crate::app::ClipboardOp::Cut => { let _ = event_tx.try_send(AppEvent::Rename(src, dest)); app.clipboard = None; }
  2189	                                    }
  2190	                                }
  2191	                            }
  2192	                            return true;
  2193	                        }
  2194	                        KeyCode::Char('a') if has_control => {
  2195	                            if let Some(fs) = app.current_file_state_mut() {
  2196	                                fs.multi_select = (0..fs.files.len()).collect();
  2197	                            }
  2198	                            return true;
  2199	                        }
  2200	                        KeyCode::Char('z') if has_control => {
  2201	                            if let Some(action) = app.undo_stack.pop() {
  2202	                                match action.clone() {
  2203	                                    crate::app::UndoAction::Rename(old, new) | crate::app::UndoAction::Move(old, new) => {
  2204	                                        let _ = std::fs::rename(&new, &old);
  2205	                                        app.redo_stack.push(action);
  2206	                                    }
  2207	                                    crate::app::UndoAction::Copy(_src, dest) => {
  2208	                                        let _ = if dest.is_dir() { std::fs::remove_dir_all(&dest) } else { std::fs::remove_file(&dest) };
  2209	                                        app.redo_stack.push(action);
  2210	                                    }
  2211	                                    _ => {}
  2212	                                }
  2213	                                for i in 0..app.panes.len() {
  2214	                                    let _ = event_tx.try_send(AppEvent::RefreshFiles(i));
  2215	                                }
  2216	                            } else {
  2217	                                // Fallback: Clear search if active
  2218	                                if let Some(fs) = app.current_file_state_mut() {
  2219	                                    if !fs.search_filter.is_empty() {
  2220	                                        fs.search_filter.clear();
  2221	                                        let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
  2222	                                    }
  2223	                                }
  2224	                            }
  2225	                            return true;
  2226	                        }
  2227	                        KeyCode::Char('y') if has_control => {
  2228	                            if let Some(action) = app.redo_stack.pop() {
  2229	                                match action.clone() {
  2230	                                    crate::app::UndoAction::Rename(old, new) | crate::app::UndoAction::Move(old, new) => {
  2231	                                        let _ = std::fs::rename(&old, &new);
  2232	                                        app.undo_stack.push(action);
  2233	                                    }
  2234	                                    crate::app::UndoAction::Copy(src, dest) => {
  2235	                                        let _ = crate::modules::files::copy_recursive(&src, &dest);
  2236	                                        app.undo_stack.push(action);
  2237	                                    }
  2238	                                    _ => {}
  2239	                                }
  2240	                                for i in 0..app.panes.len() {
  2241	                                    let _ = event_tx.try_send(AppEvent::RefreshFiles(i));
  2242	                                }
  2243	                            }
  2244	                            return true;
  2245	                        }
  2246	                        KeyCode::Left if key.modifiers.contains(KeyModifiers::ALT) => {
  2247	                            if let Some(pane) = app.panes.get_mut(app.focused_pane_index) {
  2248	                                pane.preview = None;
  2249	                                if let Some(fs) = pane.current_state_mut() { navigate_back(fs); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); } 
  2250	                            }
  2251	                            return true;
  2252	                        }
  2253	                        KeyCode::Right if key.modifiers.contains(KeyModifiers::ALT) => {
  2254	                            if let Some(pane) = app.panes.get_mut(app.focused_pane_index) {
  2255	                                pane.preview = None;
  2256	                                if let Some(fs) = pane.current_state_mut() { navigate_forward(fs); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); } 
  2257	                            }
  2258	                            return true;
  2259	                        }
  2260	                        KeyCode::Up if key.modifiers.contains(KeyModifiers::ALT) => {
  2261	                             if app.sidebar_focus {
  2262	                                  if app.sidebar_index < app.sidebar_bounds.len() {
  2263	                                      let bound = &app.sidebar_bounds[app.sidebar_index];
  2264	                                      if let SidebarTarget::Favorite(path) = &bound.target {
  2265	                                          if let Some(idx) = app.starred.iter().position(|p| p == path) {
  2266	                                              if idx > 0 {
  2267	                                                  app.starred.swap(idx, idx - 1);
  2268	                                                  if app.sidebar_index > 0 { app.sidebar_index -= 1; }
  2269	                                                  let _ = crate::config::save_state(app);
  2270	                                                  return true;
  2271	                                              }
  2272	                                          }
  2273	                                      }
  2274	                                  }
  2275	                             }
  2276	                        }
  2277	                        KeyCode::Down if key.modifiers.contains(KeyModifiers::ALT) => {
  2278	                             if app.sidebar_focus {
  2279	                                  if app.sidebar_index < app.sidebar_bounds.len() {
  2280	                                      let bound = &app.sidebar_bounds[app.sidebar_index];
  2281	                                      if let SidebarTarget::Favorite(path) = &bound.target {
  2282	                                          if let Some(idx) = app.starred.iter().position(|p| p == path) {
  2283	                                              if idx < app.starred.len() - 1 {
  2284	                                                  app.starred.swap(idx, idx + 1);
  2285	                                                  app.sidebar_index += 1;
  2286	                                                  let _ = crate::config::save_state(app);
  2287	                                                  return true;
  2288	                                              }
  2289	                                          }
  2290	                                      }
  2291	                                  }
  2292	                             }
  2293	                        }
  2294	                        KeyCode::Down => { app.move_down(key.modifiers.contains(KeyModifiers::SHIFT)); return true; }
  2295	                        KeyCode::Up => { 
  2296	                            if app.sidebar_focus {
  2297	                                if app.sidebar_index == 0 {
  2298	                                    app.mode = AppMode::Header(0);
  2299	                                    return true;
  2300	                                }
  2301	                            } else if let Some(fs) = app.current_file_state() {
  2302	                                if fs.selected_index == Some(0) || fs.files.is_empty() {
  2303	                                    app.mode = AppMode::Header(0);
  2304	                                    return true;
  2305	                                }
  2306	                            }
  2307	                            app.move_up(key.modifiers.contains(KeyModifiers::SHIFT)); 
  2308	                            return true; 
  2309	                        }
  2310	                        KeyCode::Left => { 
  2311	                            if key.modifiers.contains(KeyModifiers::SHIFT) { 
  2312	                                app.copy_to_other_pane(); 
  2313	                                let _ = event_tx.try_send(AppEvent::RefreshFiles(0)); 
  2314	                                let _ = event_tx.try_send(AppEvent::RefreshFiles(1)); 
  2315	                            } else { app.move_left(); } 
  2316	                            return true;
  2317	                        } 
  2318	                        KeyCode::Right => { 
  2319	                            if key.modifiers.contains(KeyModifiers::SHIFT) { 
  2320	                                app.copy_to_other_pane(); 
  2321	                                let _ = event_tx.try_send(AppEvent::RefreshFiles(0)); 
  2322	                                let _ = event_tx.try_send(AppEvent::RefreshFiles(1)); 
  2323	                            } else { app.move_right(); } 
  2324	                            return true;
  2325	                        } 
  2326	                        KeyCode::Enter if key.modifiers.contains(KeyModifiers::ALT) => {
  2327	                            app.mode = AppMode::Properties;
  2328	                            return true;
  2329	                        }
  2330	                        KeyCode::Enter if key.modifiers.contains(KeyModifiers::SHIFT) => {
  2331	                            let path_to_open = if let Some(fs) = app.current_file_state() {
  2332	                                if let Some(idx) = fs.selected_index {
  2333	                                    fs.files.get(idx).cloned()
  2334	                                } else { None }
  2335	                            } else { None };
  2336	
  2337	                            if let Some(path) = path_to_open {
  2338	                                if path.is_dir() {
  2339	                                    if let Some(fs) = app.current_file_state() {
  2340	                                        let new_fs = fs.clone();
  2341	                                        if let Some(pane) = app.panes.get_mut(app.focused_pane_index) {
  2342	                                            let mut fs_tab = new_fs;
  2343	                                            fs_tab.current_path = path.clone();
  2344	                                            fs_tab.selected_index = Some(0);
  2345	                                            fs_tab.history = vec![path];
  2346	                                            fs_tab.history_index = 0;
  2347	                                            pane.open_tab(fs_tab);
  2348	                                            let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
  2349	                                            return true;
  2350	                                        }
  2351	                                    }
  2352	                                }
  2353	                            }
  2354	                        }
  2355	                        KeyCode::Enter => { if let Some(fs) = app.current_file_state_mut() { if let Some(idx) = fs.selected_index { if let Some(path) = fs.files.get(idx).cloned() { if path.is_dir() { fs.current_path = path.clone(); fs.selected_index = Some(0); fs.multi_select.clear(); fs.search_filter.clear(); *fs.table_state.offset_mut() = 0; push_history(fs, path); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); } } } } return true; } 
  2356	                        KeyCode::Char(' ') => { 
  2357	                            if let Some(fs) = app.current_file_state() { 
  2358	                                if let Some(idx) = fs.selected_index { 
  2359	                                    if let Some(path) = fs.files.get(idx).cloned() { 
  2360	                                        if path.is_dir() {
  2361	                                            app.mode = AppMode::Properties;
  2362	                                        } else {
  2363	                                            let target_pane = if app.focused_pane_index == 0 { 1 } else { 0 };
  2364	                                            let _ = event_tx.try_send(AppEvent::PreviewRequested(target_pane, path));
  2365	                                        }
  2366	                                    } 
  2367	                                } 
  2368	                            } 
  2369	                            return true;
  2370	                        } 
  2371	                        KeyCode::Char('u') if has_control => {
  2372	                            if let Some(fs) = app.current_file_state_mut() {
  2373	                                if !fs.search_filter.is_empty() {
  2374	                                    fs.search_filter.clear();
  2375	                                    fs.selected_index = Some(0);
  2376	                                    *fs.table_state.offset_mut() = 0;
  2377	                                    let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
  2378	                                    return true;
  2379	                                }
  2380	                            }
  2381	                        }
  2382	                        KeyCode::Char('w') if has_control => {
  2383	                            if let Some(fs) = app.current_file_state_mut() {
  2384	                                if !fs.search_filter.is_empty() {
  2385	                                    let trimmed = fs.search_filter.trim_end();
  2386	                                    if let Some(last_space) = trimmed.rfind(' ') {
  2387	                                        fs.search_filter.truncate(last_space + 1);
  2388	                                    } else {
  2389	                                        fs.search_filter.clear();
  2390	                                    }
  2391	                                    fs.selected_index = Some(0);
  2392	                                    *fs.table_state.offset_mut() = 0;
  2393	                                    let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
  2394	                                    return true;
  2395	                                }
  2396	                            }
  2397	                        }
  2398	                        KeyCode::Backspace if has_control => {
  2399	                            if let Some(fs) = app.current_file_state_mut() {
  2400	                                if !fs.search_filter.is_empty() {
  2401	                                    let trimmed = fs.search_filter.trim_end();
  2402	                                    if let Some(last_space) = trimmed.rfind(' ') {
  2403	                                        fs.search_filter.truncate(last_space + 1);
  2404	                                    } else {
  2405	                                        fs.search_filter.clear();
  2406	                                    }
  2407	                                    fs.selected_index = Some(0);
  2408	                                    *fs.table_state.offset_mut() = 0;
  2409	                                    let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
  2410	                                    return true;
  2411	                                }
  2412	                            }
  2413	                        }
  2414	                        KeyCode::Char('l') if has_control => {
  2415	                            if let Some(fs) = app.current_file_state_mut() {
  2416	                                fs.search_filter.clear();
  2417	                                fs.selected_index = Some(0);
  2418	                                *fs.table_state.offset_mut() = 0;
  2419	                                let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
  2420	                                return true;
  2421	                            }
  2422	                        }
  2423	                        KeyCode::F(6) => {
  2424	                            let path_to_rename = if let Some(fs) = app.current_file_state() {
  2425	                                fs.selected_index.and_then(|idx| fs.files.get(idx)).cloned()
  2426	                            } else { None };
  2427	
  2428	                            if let Some(p) = path_to_rename {
  2429	                                app.mode = AppMode::Rename;
  2430	                                app.input.set_value(p.file_name().unwrap().to_string_lossy().to_string());
  2431	                                app.rename_selected = true;
  2432	                                return true;
  2433	                            }
  2434	                        }
  2435	                        KeyCode::Delete => {
  2436	                            if let Some(fs) = app.current_file_state() {
  2437	                                if fs.selected_index.is_some() {
  2438	                                    if !app.confirm_delete {
  2439	                                        let mut paths_to_delete = Vec::new();
  2440	                                        if !fs.multi_select.is_empty() {
  2441	                                            for &idx in &fs.multi_select {
  2442	                                                if let Some(p) = fs.files.get(idx) { paths_to_delete.push(p.clone()); }
  2443	                                            }
  2444	                                        } else if let Some(idx) = fs.selected_index {
  2445	                                            if let Some(path) = fs.files.get(idx) {
  2446	                                                paths_to_delete.push(path.clone());
  2447	                                            }
  2448	                                        }
  2449	
  2450	                                        for p in paths_to_delete {
  2451	                                            let _ = event_tx.try_send(AppEvent::Delete(p));
  2452	                                        }
  2453	                                    } else {
  2454	                                        app.mode = AppMode::Delete;
  2455	                                    }
  2456	                                    return true;
  2457	                                }
  2458	                            }
  2459	                        }
  2460	                        KeyCode::Char('~') if key.modifiers.is_empty() => {
  2461	                            if let Some(fs) = app.current_file_state_mut() {
  2462	                                if let Some(home) = dirs::home_dir() {
  2463	                                    fs.current_path = home.clone();
  2464	                                    fs.selected_index = Some(0);
  2465	                                    fs.multi_select.clear();
  2466	                                    *fs.table_state.offset_mut() = 0;
  2467	                                    push_history(fs, home);
  2468	                                    let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
  2469	                                    return true;
  2470	                                }
  2471	                            }
  2472	                        }
  2473	                        KeyCode::Char(c) if key.modifiers.is_empty() => { if let Some(fs) = app.current_file_state_mut() { fs.search_filter.push(c); fs.selected_index = Some(0); *fs.table_state.offset_mut() = 0; let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); } return true; } 
  2474	                        KeyCode::Backspace => { if let Some(fs) = app.current_file_state_mut() { if !fs.search_filter.is_empty() { fs.search_filter.pop(); fs.selected_index = Some(0); *fs.table_state.offset_mut() = 0; let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); } else if let Some(parent) = fs.current_path.parent() { let p = parent.to_path_buf(); fs.current_path = p.clone(); fs.selected_index = Some(0); fs.multi_select.clear(); *fs.table_state.offset_mut() = 0; push_history(fs, p); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); } } return true; } 
  2475	                        _ => { return false; } 
  2476	                    }
  2477	                }
  2478	            }
  2479	        }
  2480	        Event::Mouse(me) => {
  2481	            let column = me.column;
  2482	            let row = me.row;
  2483	            let (w, h) = app.terminal_size;
  2484	
  2485	            // 0. Modal Handling (Highest Priority)
  2486	            // If we are in a modal mode, it MUST consume the event or close.
  2487	            match app.mode.clone() {
  2488	                AppMode::Highlight => {
  2489	                    if let MouseEventKind::Down(_) = me.kind {
  2490	                        let area_w = 34; let area_h = 5; let area_x = (w.saturating_sub(area_w)) / 2; let area_y = (h.saturating_sub(area_h)) / 2;
  2491	                        if column >= area_x && column < area_x + area_w && row >= area_y && row < area_y + area_h {
  2492	                            let rel_x = column.saturating_sub(area_x + 3);
  2493	                            let rel_y = row.saturating_sub(area_y + 2);
  2494	                            if rel_y == 0 || rel_y == 1 {
  2495	                                let colors = [1, 2, 3, 4, 5, 6, 0];
  2496	                                let color_idx_raw = (rel_x / 4) as usize;
  2497	                                if color_idx_raw < colors.len() {
  2498	                                    let color_code = colors[color_idx_raw];
  2499	                                    let color = if color_code == 0 { None } else { Some(color_code) };
  2500	                                    if let Some(fs) = app.current_file_state() {
  2501	                                        let mut paths = Vec::new();
  2502	                                        if !fs.multi_select.is_empty() {
  2503	                                            for &idx in &fs.multi_select { if let Some(p) = fs.files.get(idx) { paths.push(p.clone()); } }
  2504	                                        } else if let Some(idx) = fs.selected_index {
  2505	                                            if let Some(p) = fs.files.get(idx) { paths.push(p.clone()); }
  2506	                                        }
  2507	                                        for p in paths { if let Some(col) = color { app.path_colors.insert(p, col); } else { app.path_colors.remove(&p); } }
  2508	                                        let _ = crate::config::save_state(app);
  2509	                                    }
  2510	                                    app.mode = AppMode::Normal;
  2511	                                }
  2512	                            }
  2513	                        } else { app.mode = AppMode::Normal; }
  2514	                    }
  2515	                    return true; 
  2516	                }
  2517	                AppMode::Settings => {
  2518	                    if let MouseEventKind::Down(_) = me.kind {
  2519	                        let area_w = (w as f32 * 0.8) as u16; let area_h = (h as f32 * 0.8) as u16; let area_x = (w - area_w) / 2; let area_y = (h - area_h) / 2;
  2520	                        if column >= area_x && column < area_x + area_w && row >= area_y && row < area_y + area_h {
  2521	                            let inner = ratatui::layout::Rect::new(area_x + 1, area_y + 1, area_w.saturating_sub(2), area_h.saturating_sub(2));
  2522	                            if column < inner.x + 15 {
  2523	                                let rel_y = row.saturating_sub(inner.y);
  2524	                                match rel_y {
  2525	                                    0 => app.settings_section = SettingsSection::Columns,
  2526	                                    1 => app.settings_section = SettingsSection::Tabs,
  2527	                                    2 => app.settings_section = SettingsSection::General,
  2528	                                    3 => app.settings_section = SettingsSection::Remotes,
  2529	                                    4 => app.settings_section = SettingsSection::Shortcuts,
  2530	                                    _ => {} 
  2531	                                }
  2532	                            } else {
  2533	                                match app.settings_section {
  2534	                                    SettingsSection::Columns => {
  2535	                                        if row >= inner.y && row < inner.y + 3 {
  2536	                                            let content_x = column.saturating_sub(inner.x + 15);
  2537	                                            if content_x < 12 { app.settings_target = SettingsTarget::SingleMode; } else if content_x < 25 { app.settings_target = SettingsTarget::SplitMode; }
  2538	                                        } else if row >= inner.y + 4 {
  2539	                                            let rel_y = row.saturating_sub(inner.y + 4);
  2540	                                            match rel_y { 
  2541	                                                0 => app.toggle_column(crate::app::FileColumn::Size), 
  2542	                                                1 => app.toggle_column(crate::app::FileColumn::Modified), 
  2543	                                                2 => app.toggle_column(crate::app::FileColumn::Permissions), 
  2544	                                                _ => {} 
  2545	                                            }
  2546	                                            let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
  2547	                                        }
  2548	                                    }
  2549	                                    SettingsSection::General => {
  2550	                                        let rel_y = row.saturating_sub(inner.y + 1);
  2551	                                        match rel_y { 
  2552	                                            0 => app.default_show_hidden = !app.default_show_hidden, 
  2553	                                            1 => app.confirm_delete = !app.confirm_delete, 
  2554	                                            2 => {
  2555	                                                app.icon_mode = match app.icon_mode {
  2556	                                                    IconMode::Nerd => IconMode::Unicode,
  2557	                                                    IconMode::Unicode => IconMode::ASCII,
  2558	                                                    IconMode::ASCII => IconMode::Nerd,
  2559	                                                };
  2560	                                            }
  2561	                                            _ => {} 
  2562	                                        }
  2563	                                    }
  2564	                                    _ => {} 
  2565	                                }
  2566	                            }
  2567	                        } else { app.mode = AppMode::Normal; }
  2568	                    }
  2569	                    return true;
  2570	                }
  2571	                AppMode::ImportServers => {
  2572	                    if let MouseEventKind::Down(_) = me.kind {
  2573	                        let area_w = (w as f32 * 0.6) as u16; let area_h = (h as f32 * 0.2) as u16; let area_x = (w - area_w) / 2; let area_y = (h - area_h) / 2;
  2574	                        if !(column >= area_x && column < area_x + area_w && row >= area_y && row < area_y + area_h) {
  2575	                            let mut handled = false;
  2576	                            if row >= 3 {
  2577	                                let index = fs_mouse_index(row, app);
  2578	                                if let Some(fs) = app.current_file_state() { if index < fs.files.len() { let path = &fs.files[index]; if path.extension().map(|e| e == "toml").unwrap_or(false) { app.input.set_value(path.file_name().unwrap_or_default().to_string_lossy().to_string()); handled = true; } } } 
  2579	                            }
  2580	                            if !handled { app.mode = AppMode::Normal; }
  2581	                        }
  2582	                    }
  2583	                    return true;
  2584	                }
  2585	                AppMode::NewFile | AppMode::NewFolder | AppMode::Rename | AppMode::Delete | AppMode::Properties | AppMode::CommandPalette | AppMode::AddRemote(_) | AppMode::OpenWith(_) => {
  2586	                    if let MouseEventKind::Down(_) = me.kind {
  2587	                        let (area_w, area_h) = match app.mode {
  2588	                            AppMode::NewFile | AppMode::NewFolder | AppMode::Rename | AppMode::Delete => ((w as f32 * 0.4) as u16, (h as f32 * 0.1) as u16),
  2589	                            AppMode::Properties => ((w as f32 * 0.5) as u16, (h as f32 * 0.5) as u16),
  2590	                            AppMode::CommandPalette => ((w as f32 * 0.6) as u16, (h as f32 * 0.2) as u16),
  2591	                            AppMode::AddRemote(_) => ((w as f32 * 0.6) as u16, (h as f32 * 0.4) as u16),
  2592	                            AppMode::OpenWith(_) => ((w as f32 * 0.6) as u16, (h as f32 * 0.2) as u16),
  2593	                            _ => (0, 0)
  2594	                        };
  2595	                        let area_x = (w - area_w) / 2; let area_y = (h - area_h) / 2;
  2596	                        if column < area_x || column >= area_x + area_w || row < area_y || row >= area_y + area_h {
  2597	                            app.mode = AppMode::Normal; app.input.clear();
  2598	                        }
  2599	                    }
  2600	                    return true;
  2601	                }
  2602	                AppMode::ConfirmReset => {
  2603	                    if let MouseEventKind::Down(_) = me.kind {
  2604	                        let area_w = (w as f32 * 0.4) as u16; let area_h = (h as f32 * 0.1) as u16; let area_x = (w - area_w) / 2; let area_y = (h - area_h) / 2;
  2605	                        if column < area_x || column >= area_x + area_w || row < area_y || row >= area_y + area_h {
  2606	                            app.mode = AppMode::Normal;
  2607	                        }
  2608	                    }
  2609	                    return true;
  2610	                }
  2611	                AppMode::ContextMenu { x, y, target, actions } => {
  2612	                    if let MouseEventKind::Down(_) = me.kind {
  2613	                        let menu_width = 25; 
  2614	                        let menu_height = actions.len() as u16 + 2;
  2615	                        let mut draw_x = x; let mut draw_y = y;
  2616	                        if draw_x + menu_width > w { draw_x = w.saturating_sub(menu_width); }
  2617	                        if draw_y + menu_height > h { draw_y = h.saturating_sub(menu_height); }
  2618	
  2619	                        if column >= draw_x && column < draw_x + menu_width && row >= draw_y && row < draw_y + menu_height {
  2620	                            if row > draw_y && row < draw_y + menu_height - 1 {
  2621	                                let menu_row = (row - draw_y - 1) as usize;
  2622	                                if let Some(action) = actions.get(menu_row) { handle_context_menu_action(action, &target, app, event_tx.clone()); }
  2623	                            }
  2624	                        } else { app.mode = AppMode::Normal; }
  2625	                    }
  2626	                    return true;
  2627	                }
  2628	                _ => {}
  2629	            }
  2630	
  2631	            match me.kind {
  2632	                MouseEventKind::Down(button) => {
  2633	                    crate::app::log_debug(&format!("MOUSE DOWN: button={:?} row={} col={}", button, row, column));
  2634	                    
  2635	                    let sidebar_width = app.sidebar_width();
  2636	                    
  2637	                    // Check Header Icons
  2638	                    if row == 0 {
  2639	                        if let Some((_, action_id)) = app.header_icon_bounds.iter().find(|(rect, _)| {
  2640	                            column >= rect.x && column < rect.x + rect.width && row == rect.y
  2641	                        }) {
  2642	                            match action_id.as_str() {
  2643	                                "back" => {
  2644	                                    if let Some(fs) = app.current_file_state_mut() {
  2645	                                        navigate_back(fs);
  2646	                                        let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
  2647	                                    }
  2648	                                }
  2649	                                "forward" => {
  2650	                                    if let Some(fs) = app.current_file_state_mut() {
  2651	                                        navigate_forward(fs);
  2652	                                        let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
  2653	                                    }
  2654	                                }
  2655	                                "split" => {
  2656	                                    app.toggle_split();
  2657	                                    let _ = event_tx.try_send(AppEvent::RefreshFiles(0));
  2658	                                    let _ = event_tx.try_send(AppEvent::RefreshFiles(1));
  2659	                                }
  2660	                                "burger" => {
  2661	                                    app.mode = AppMode::Settings;
  2662	                                }
  2663	                                "reset" => {
  2664	                                    app.mode = AppMode::ConfirmReset;
  2665	                                }
  2666	                                _ => {}
  2667	                            }
  2668	                            return true;
  2669	                        }
  2670	                    }
  2671	
  2672	                    if button == MouseButton::Left && column >= sidebar_width.saturating_sub(1) && column <= sidebar_width && row >= 1 {
  2673	                        app.is_resizing_sidebar = true;
  2674	                        return true;
  2675	                    }
  2676	
  2677	                    // 1. Header handling (Row 0) - Tabs & Settings
  2678	                    if row == 0 {
  2679	                        let clicked_tab = app.tab_bounds.iter().find(|(rect, _, _)| rect.contains(ratatui::layout::Position { x: column, y: row })).cloned();
  2680	                        if let Some((_, p_idx, t_idx)) = clicked_tab {
  2681	                            if button == MouseButton::Left {
  2682	                                if let Some(pane) = app.panes.get_mut(p_idx) { pane.active_tab_index = t_idx; app.focused_pane_index = p_idx; app.sidebar_focus = false; let _ = event_tx.try_send(AppEvent::RefreshFiles(p_idx)); }
  2683	                            } else if button == MouseButton::Right {
  2684	                                if let Some(pane) = app.panes.get_mut(p_idx) {
  2685	                                    if pane.tabs.len() > 1 { pane.tabs.remove(t_idx); if pane.active_tab_index >= pane.tabs.len() { pane.active_tab_index = pane.tabs.len() - 1; } let _ = event_tx.try_send(AppEvent::RefreshFiles(p_idx)); }
  2686	                                }
  2687	                            }
  2688	                            return true;
  2689	                        }
  2690	                        if column < 10 { app.mode = AppMode::Settings; return true; }
  2691	                        if column >= w.saturating_sub(3) { app.toggle_split(); let _ = event_tx.try_send(AppEvent::RefreshFiles(0)); let _ = event_tx.try_send(AppEvent::RefreshFiles(1)); return true; }
  2692	                    }
  2693	
  2694	                    // Check Breadcrumbs
  2695	                    for (p_idx, pane) in app.panes.iter_mut().enumerate() {
  2696	                        if let Some(fs) = pane.current_state_mut() {
  2697	                            let clicked_crumb = fs.breadcrumb_bounds.iter().find(|(rect, _)| rect.contains(ratatui::layout::Position { x: column, y: row })).map(|(_, path)| path.clone());
  2698	                            if let Some(path) = clicked_crumb {
  2699	                                if button == MouseButton::Middle {
  2700	                                    let mut new_fs = fs.clone();
  2701	                                    new_fs.current_path = path.clone();
  2702	                                    new_fs.selected_index = Some(0);
  2703	                                    new_fs.search_filter.clear();
  2704	                                    *new_fs.table_state.offset_mut() = 0;
  2705	                                    new_fs.history = vec![path];
  2706	                                    new_fs.history_index = 0;
  2707	                                    pane.open_tab(new_fs);
  2708	                                } else {
  2709	                                    fs.current_path = path.clone(); fs.selected_index = Some(0); fs.multi_select.clear(); fs.search_filter.clear(); *fs.table_state.offset_mut() = 0; push_history(fs, path);
  2710	                                }
  2711	                                let _ = event_tx.try_send(AppEvent::RefreshFiles(p_idx)); app.focused_pane_index = p_idx; app.sidebar_focus = false; return true;
  2712	                            }
  2713	                        }
  2714	                    }
  2715	
  2716	                    // Update pane focus
  2717	                    if column >= sidebar_width {
  2718	                        let content_area_width = w.saturating_sub(sidebar_width);
  2719	                        let pane_count = app.panes.len();
  2720	                        let pane_width = if pane_count > 0 { content_area_width / pane_count as u16 } else { content_area_width };
  2721	                        let clicked_pane = (column.saturating_sub(sidebar_width) / pane_width) as usize;
  2722	                        if clicked_pane < pane_count {
  2723	                            // Check if clicking on column headers for resizing
  2724	                            if row == 1 || row == 2 {
  2725	                                let mut handled_resize = false;
  2726	                                if let Some(pane) = app.panes.get(clicked_pane) {
  2727	                                    if let Some(fs) = pane.current_state() {
  2728	                                        for (rect, col) in &fs.column_bounds {
  2729	                                            // The rect should already be absolute from draw logic
  2730	                                            if column >= rect.x && column < rect.x + rect.width + 1 {
  2731	                                                app.is_resizing_column = Some((clicked_pane, *col));
  2732	                                                app.initial_col_width = rect.width;
  2733	                                                app.drag_start_pos = Some((column, row));
  2734	                                                handled_resize = true;
  2735	                                                let _ = event_tx.try_send(AppEvent::StatusMsg(format!("Resizing column: {:?}", col)));
  2736	                                                break;
  2737	                                            }
  2738	                                        }
  2739	                                    }
  2740	                                }
  2741	                                if handled_resize { return true; }
  2742	                            }
  2743	                            app.focused_pane_index = clicked_pane; app.sidebar_focus = false; 
  2744	                        }
  2745	                    }
  2746	
  2747	                    // Footer interaction
  2748	                    if row == h.saturating_sub(1) {
  2749	                        let current_x = 0;
  2750	                        if column >= current_x && column < current_x + 9 { app.running = false; return true; } 
  2751	                        // Skip some spacing or log area if needed, but let's just handle basic buttons
  2752	                        if column < 50 { // Rough estimate for left side
  2753	                             // Quit button is handled above
  2754	                        }
  2755	                    }
  2756	
  2757	                    if column < sidebar_width {
  2758	                        app.sidebar_focus = true;
  2759	                        app.drag_start_pos = Some((column, row));
  2760	                        let clicked_sidebar_item = app.sidebar_bounds.iter().find(|b| b.y == row).cloned();
  2761	                        if let Some(bound) = clicked_sidebar_item {
  2762	                            app.sidebar_index = bound.index;
  2763	                            if let SidebarTarget::Favorite(ref p) = bound.target {
  2764	                                app.drag_source = Some(p.clone());
  2765	                            }
  2766	                            if button == MouseButton::Right {
  2767	                                let target = match &bound.target {
  2768	                                    SidebarTarget::Favorite(p) => Some(ContextMenuTarget::SidebarFavorite(p.clone())),
  2769	                                    SidebarTarget::Remote(idx) => Some(ContextMenuTarget::SidebarRemote(*idx)),
  2770	                                    SidebarTarget::Storage(idx) => Some(ContextMenuTarget::SidebarStorage(*idx)),
  2771	                                    _ => None
  2772	                                };
  2773	                                if let Some(t) = target { 
  2774	                                    let actions = get_context_menu_actions(&t, app);
  2775	                                    app.mode = AppMode::ContextMenu { x: column, y: row, target: t, actions }; 
  2776	                                    return true; 
  2777	                                }
  2778	                            }
  2779	                        }
  2780	                        return true;
  2781	                    }
  2782	                    
  2783	                    if row >= 3 {
  2784	                        let index = fs_mouse_index(row, app);
  2785	                        let mut selected_path = None; let mut is_dir = false;
  2786	                        let has_modifiers = me.modifiers.contains(KeyModifiers::SHIFT) || me.modifiers.contains(KeyModifiers::CONTROL);
  2787	                        
  2788	                        if let Some(fs) = app.current_file_state_mut() {
  2789	                            if index < fs.files.len() {
  2790	                                if fs.files[index].to_string_lossy() == "__DIVIDER__" { return true; } 
  2791	                                
  2792	                                if button == MouseButton::Left {
  2793	                                    if me.modifiers.contains(KeyModifiers::CONTROL) {
  2794	                                        // Toggle individual
  2795	                                        if fs.multi_select.contains(&index) {
  2796	                                            fs.multi_select.remove(&index);
  2797	                                        } else {
  2798	                                            fs.multi_select.insert(index);
  2799	                                        }
  2800	                                        fs.selected_index = Some(index);
  2801	                                        fs.table_state.select(Some(index));
  2802	                                    } else if me.modifiers.contains(KeyModifiers::SHIFT) {
  2803	                                        // Range select
  2804	                                        let anchor = fs.selection_anchor.unwrap_or(fs.selected_index.unwrap_or(0));
  2805	                                        fs.multi_select.clear();
  2806	                                        let start = std::cmp::min(anchor, index);
  2807	                                        let end = std::cmp::max(anchor, index);
  2808	                                        for i in start..=end {
  2809	                                            fs.multi_select.insert(i);
  2810	                                        }
  2811	                                        fs.selected_index = Some(index);
  2812	                                        fs.table_state.select(Some(index));
  2813	                                    } else {
  2814	                                        // Normal click
  2815	                                        fs.multi_select.clear();
  2816	                                        fs.selection_anchor = Some(index);
  2817	                                        fs.selected_index = Some(index);
  2818	                                        fs.table_state.select(Some(index));
  2819	                                    }
  2820	                                } else {
  2821	                                    // Right click: if already part of selection, don't clear
  2822	                                    if !fs.multi_select.contains(&index) {
  2823	                                        fs.multi_select.clear();
  2824	                                        fs.selected_index = Some(index);
  2825	                                        fs.table_state.select(Some(index));
  2826	                                    }
  2827	                                }
  2828	                                
  2829	                                let p = fs.files[index].clone(); is_dir = fs.metadata.get(&p).map(|m| m.is_dir).unwrap_or(false); selected_path = Some(p);
  2830	                            } else {
  2831	                                // Clicked on empty space
  2832	                                if button == MouseButton::Left && !has_modifiers {
  2833	                                    fs.selected_index = None;
  2834	                                    fs.table_state.select(None);
  2835	                                    fs.multi_select.clear();
  2836	                                    fs.selection_anchor = None;
  2837	                                }
  2838	                                if button == MouseButton::Right { 
  2839	                                    let target = ContextMenuTarget::EmptySpace;
  2840	                                    let actions = get_context_menu_actions(&target, app);
  2841	                                    app.mode = AppMode::ContextMenu { x: column, y: row, target, actions }; 
  2842	                                    return true; 
  2843	                                } 
  2844	                            }
  2845	                        }
  2846	                        if let Some(path) = selected_path {
  2847	                            if button == MouseButton::Right { 
  2848	                                let target = if is_dir { ContextMenuTarget::Folder(index) } else { ContextMenuTarget::File(index) }; 
  2849	                                let actions = get_context_menu_actions(&target, app);
  2850	                                app.mode = AppMode::ContextMenu { x: column, y: row, target, actions }; 
  2851	                                return true; 
  2852	                            }
  2853	                            if button == MouseButton::Middle {
  2854	                                if is_dir {
  2855	                                    if let Some(pane) = app.panes.get_mut(app.focused_pane_index) {
  2856	                                        if let Some(fs) = pane.current_state() {
  2857	                                            let mut new_fs = fs.clone();
  2858	                                            new_fs.current_path = path.clone();
  2859	                                            new_fs.selected_index = Some(0);
  2860	                                            new_fs.search_filter.clear();
  2861	                                            *new_fs.table_state.offset_mut() = 0;
  2862	                                            new_fs.history = vec![path.clone()];
  2863	                                            new_fs.history_index = 0;
  2864	                                            pane.open_tab(new_fs);
  2865	                                            let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
  2866	                                        }
  2867	                                    }
  2868	                                } else {
  2869	                                    let target_pane = if app.focused_pane_index == 0 { 1 } else { 0 };
  2870	                                    let _ = event_tx.try_send(AppEvent::PreviewRequested(target_pane, path.clone()));
  2871	                                }
  2872	                                return true;
  2873	                            }
  2874	                            app.drag_source = Some(path.clone()); app.drag_start_pos = Some((column, row));
  2875	                            // Double click detection
  2876	                            if button == MouseButton::Left && app.mouse_last_click.elapsed() < Duration::from_millis(500) && app.mouse_click_pos == (column, row) {
  2877	                                if path.is_dir() { if let Some(fs) = app.current_file_state_mut() { fs.current_path = path.clone(); fs.selected_index = Some(0); fs.multi_select.clear(); fs.search_filter.clear(); *fs.table_state.offset_mut() = 0; push_history(fs, path); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); } } 
  2878	                                else { spawn_detached("xdg-open", vec![&path.to_string_lossy()]); } 
  2879	                            }
  2880	                            app.mouse_last_click = std::time::Instant::now(); app.mouse_click_pos = (column, row);
  2881	                        }
  2882	                    } else if row >= 1 && button == MouseButton::Right { // Right click above file list but below header
  2883	                        let target = ContextMenuTarget::EmptySpace;
  2884	                        let actions = get_context_menu_actions(&target, app);
  2885	                        app.mode = AppMode::ContextMenu { x: column, y: row, target, actions };
  2886	                        return true;
  2887	                    }
  2888	                }
  2889	                                MouseEventKind::Up(_) => {
  2890	                                    if let Some((pane_idx, col)) = app.is_resizing_column.take() {
  2891	                                        let mut is_click = true;
  2892	                                        if let Some((sx, _)) = app.drag_start_pos {
  2893	                                            if (column as i16 - sx as i16).abs() > 1 {
  2894	                                                is_click = false;
  2895	                                            }
  2896	                                        }
  2897	
  2898	                                        if is_click {
  2899	                                            if let Some(pane) = app.panes.get_mut(pane_idx) {
  2900	                                                if let Some(fs) = pane.current_state_mut() {
  2901	                                                    if fs.sort_column == col {
  2902	                                                        fs.sort_ascending = !fs.sort_ascending;
  2903	                                                    } else {
  2904	                                                        fs.sort_column = col;
  2905	                                                        fs.sort_ascending = true;
  2906	                                                    }
  2907	                                                    let _ = event_tx.try_send(AppEvent::RefreshFiles(pane_idx));
  2908	                                                }
  2909	                                            }
  2910	                                        }
  2911	
  2912	                                        app.is_dragging = false;
  2913	                                        app.drag_start_pos = None;
  2914	                                        let _ = crate::config::save_state(app);
  2915	                                        return true;
  2916	                                    }
  2917	
  2918	                                    if app.is_resizing_sidebar {
  2919	                                        app.is_resizing_sidebar = false;
  2920	                                        let _ = crate::config::save_state(app);
  2921	                                        return true;
  2922	                                    }
  2923	
  2924	                                    if app.is_dragging {
  2925	                                        let mut reorder_done = false;
  2926	                                        if let Some((sx, _)) = app.drag_start_pos {
  2927	                                            if sx < app.sidebar_width() {
  2928	                                                // Reordering handled in Drag event
  2929	                                                let _ = crate::config::save_state(app);
  2930	                                                reorder_done = true; 
  2931	                                            }
  2932	                                        }
  2933	
  2934	                                        if !reorder_done {
  2935	                                            if let Some(source) = &app.drag_source {
  2936	                                                if let Some(target) = &app.hovered_drop_target {
  2937	                                                    match target {
  2938	                                                        DropTarget::ImportServers | DropTarget::RemotesHeader => {
  2939	                                                            if source.extension().map(|e| e == "toml").unwrap_or(false) {
  2940	                                                                let _ = app.import_servers(source.clone());
  2941	                                                                let _ = crate::config::save_state(app);
  2942	                                                                app.mode = AppMode::Normal;
  2943	                                                            }
  2944	                                                        }
  2945	                                                        DropTarget::Favorites => {
  2946	                                                            if source.is_dir() { if !app.starred.contains(source) { app.starred.push(source.clone()); let _ = crate::config::save_state(app); } } // Add to favorites if it's a directory
  2947	                                                        }
  2948	                                                        DropTarget::Pane(target_pane_idx) => {
  2949	                                                            if let Some(dest_path) = app.panes.get(*target_pane_idx).and_then(|p| p.current_state()).map(|fs| fs.current_path.clone()) {
  2950	                                                                if let Some(filename) = source.file_name() {
  2951	                                                                    let dest = dest_path.join(filename);
  2952	                                                                    if me.modifiers.contains(KeyModifiers::SHIFT) {
  2953	                                                                        let _ = event_tx.try_send(AppEvent::Copy(source.clone(), dest));
  2954	                                                                    } else {
  2955	                                                                        let _ = event_tx.try_send(AppEvent::Rename(source.clone(), dest));
  2956	                                                                    }
  2957	                                                                }
  2958	                                                            }
  2959	                                                        }
  2960	                                                        _ => {} 
  2961	                                                    }
  2962	                                                }
  2963	                                            }
  2964	                                        }
  2965	                                    } else {
  2966	                                        if column < app.sidebar_width() {
  2967	                                            if let Some(bound) = app.sidebar_bounds.iter().find(|b| b.y == row).cloned() {
  2968	                                                match bound.target {
  2969	                                                    SidebarTarget::Header(h) if h == "REMOTES" => { app.mode = AppMode::ImportServers; app.input.set_value("servers.toml".to_string()); }
  2970	                                                    SidebarTarget::Favorite(p) => { let p2 = p.clone(); if let Some(fs) = app.current_file_state_mut() { fs.current_path = p2.clone(); fs.remote_session = None; fs.selected_index = Some(0); fs.multi_select.clear(); fs.search_filter.clear(); *fs.table_state.offset_mut() = 0; push_history(fs, p2); } let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); app.sidebar_focus = true; }
  2971	                                                    SidebarTarget::Storage(idx) => {
  2972	                                                        if let Some(disk) = app.system_state.disks.get(idx) {
  2973	                                                            if !disk.is_mounted {
  2974	                                                                let dev = disk.device.clone(); let tx = event_tx.clone(); let pane_idx = app.focused_pane_index;
  2975	                                                                tokio::spawn(async move { if let Ok(out) = std::process::Command::new("udisksctl").arg("mount").arg("-b").arg(&dev).output() { if let Some(_) = String::from_utf8_lossy(&out.stdout).split(" at ").last() { tokio::time::sleep(Duration::from_millis(200)).await; let _ = tx.send(AppEvent::RefreshFiles(pane_idx)).await; } } });
  2976	                                                            } else {
  2977	                                                                let p = std::path::PathBuf::from(&disk.name);
  2978	                                                                if let Some(fs) = app.current_file_state_mut() { fs.current_path = p.clone(); fs.remote_session = None; fs.selected_index = Some(0); fs.search_filter.clear(); *fs.table_state.offset_mut() = 0; push_history(fs, p); }
  2979	                                                                let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); app.sidebar_focus = false;
  2980	                                                            }
  2981	                                                        }
  2982	                                                    }
  2983	                                                    SidebarTarget::Remote(idx) => { execute_command(crate::app::CommandAction::ConnectToRemote(idx), app, event_tx.clone()); app.sidebar_focus = false; }
  2984	                                                    _ => {} 
  2985	                                                }
  2986	                                            }
  2987	                                        }
  2988	                                    }
  2989	                                    app.is_dragging = false; app.drag_start_pos = None; app.drag_source = None; app.hovered_drop_target = None;
  2990	                                }
  2991	                                                                                                                    MouseEventKind::Moved | MouseEventKind::Drag(_) => {
  2992	                                                                                                                        app.mouse_pos = (column, row);
  2993	                                                                                                    
  2994	                                                                                                                        if let Some((pane_idx, col)) = app.is_resizing_column {
  2995	                                                                                                                            if let Some((sx, _)) = app.drag_start_pos {
  2996	                                                                                                                                let delta = column as i16 - sx as i16;
  2997	                                                                                                                                let new_width = (app.initial_col_width as i16 + delta).max(2).min(100) as u16;
  2998	                                                                                                                                
  2999	                                                                                                                                if let Some(pane) = app.panes.get_mut(pane_idx) {
  3000	                                                                                                                                    if let Some(fs) = pane.current_state_mut() {
  3001	                                                                                                                                        fs.column_widths.insert(col, new_width);
  3002	                                                                                                                                    }
  3003	                                                                                                                                }
  3004	                                                                                                                            }
  3005	                                                                                                                            return true;
  3006	                                                                                                                        }
  3007	                                                                                                    
  3008	                                                                                                                        if app.is_resizing_sidebar {
  3009	                                                                                                                            let (w, _) = app.terminal_size;
  3010	                                                                                                                            if w > 0 {
  3011	                                                                                                                                let new_percent = (column as f32 / w as f32 * 100.0) as u16;
  3012	                                                                                                                                app.sidebar_width_percent = new_percent.clamp(5, 50);
  3013	                                                                                                                            }
  3014	                                                                                                                            return true;
  3015	                                                                                                                        }
  3016	                                                                                                    
  3017	                                                                                                                        // Check if drag has started
  3018	                                                                                                                        if let Some((sx, sy)) = app.drag_start_pos { 
  3019	                                                                                                                            if ((column as i16 - sx as i16).pow(2) + (row as i16 - sy as i16).pow(2)) as f32 >= 1.0 { 
  3020	                                                                                                                                app.is_dragging = true; 
  3021	                                                                                                                            } 
  3022	                                                                                                                        }
  3023	                                                                                                    
  3024	                                                                                                                        if app.is_dragging {
  3025	                                                                                                                            let sidebar_width = app.sidebar_width();
  3026	                                                                                                                            
  3027	                                                                                                                            // Live Reorder Logic
  3028	                                                                                                                            if let Some((sx, _)) = app.drag_start_pos {
  3029	                                                                                                                                if sx < sidebar_width {
  3030	                                                                                                                                    if let Some(source_path) = &app.drag_source {
  3031	                                                                                                                                        if let Some(hovered_bound) = app.sidebar_bounds.iter().find(|b| b.y == row).cloned() {
  3032	                                                                                                                                            if let SidebarTarget::Favorite(target_path) = hovered_bound.target {
  3033	                                                                                                                                                if source_path != &target_path {
  3034	                                                                                                                                                    if let Some(s_idx) = app.starred.iter().position(|p| p == source_path) {
  3035	                                                                                                                                                        if let Some(e_idx) = app.starred.iter().position(|p| p == &target_path) {
  3036	                                                                                                                                                            let item = app.starred.remove(s_idx);
  3037	                                                                                                                                                            app.starred.insert(e_idx, item);
  3038	                                                                                                                                                            app.sidebar_index = hovered_bound.index;
  3039	                                                                                                                                                        }
  3040	                                                                                                                                                    }
  3041	                                                                                                                                                }
  3042	                                                                                                                                            }
  3043	                                                                                                                                        }
  3044	                                                                                                                                    }
  3045	                                                                                                                                }
  3046	                                                                                                                            }
  3047	                                                                                                    
  3048	                                                                                                                            if app.mode == AppMode::ImportServers {
  3049	                                                                                                                                let area_w = (w as f32 * 0.6) as u16; let area_h = (h as f32 * 0.2) as u16; let area_x = (w - area_w) / 2; let area_y = (h - area_h) / 2;
  3050	                                                                                                                                if column >= area_x && column < area_x + area_w && row >= area_y && row < area_y + area_h { app.hovered_drop_target = Some(DropTarget::ImportServers); } else { app.hovered_drop_target = None; }
  3051	                                                                                                                            } else {
  3052	                                                                                                                                if column < sidebar_width {
  3053	                                                                                                                                    if let Some(bound) = app.sidebar_bounds.iter().find(|b| b.y == row) {
  3054	                                                                                                                                        if let SidebarTarget::Header(h) = &bound.target {
  3055	                                                                                                                                            if h == "REMOTES" { app.hovered_drop_target = Some(DropTarget::RemotesHeader); } else { app.hovered_drop_target = Some(DropTarget::Favorites); }
  3056	                                                                                                                                        } else { app.hovered_drop_target = Some(DropTarget::Favorites); }
  3057	                                                                                                                                    } else { app.hovered_drop_target = Some(DropTarget::Favorites); }
  3058	                                                                                                                                } else {
  3059	                                                                                                                                    let content_area_width = w.saturating_sub(sidebar_width);
  3060	                                                                                                                                    let pane_count = app.panes.len();
  3061	                                                                                                                                    if pane_count > 1 {
  3062	                                                                                                                                        let pane_width = content_area_width / pane_count as u16;
  3063	                                                                                                                                        let hovered_pane_idx = (column.saturating_sub(sidebar_width) / pane_width) as usize;
  3064	                                                                                                                                        if hovered_pane_idx < pane_count && hovered_pane_idx != app.focused_pane_index {
  3065	                                                                                                                                            app.hovered_drop_target = Some(DropTarget::Pane(hovered_pane_idx));
  3066	                                                                                                                                        } else { app.hovered_drop_target = None; }
  3067	                                                                                                                                    } else { app.hovered_drop_target = None; }
  3068	                                                                                                                                }
  3069	                                                                                                                            }
  3070	                                                                                                                        }
  3071	                                                                                                                        return true;
  3072	                                                                                                                    }                                MouseEventKind::ScrollUp => { if let Some(fs) = app.current_file_state_mut() { let new_offset = fs.table_state.offset().saturating_sub(3); *fs.table_state.offset_mut() = new_offset; } return true; } 
  3073	                MouseEventKind::ScrollDown => { if let Some(fs) = app.current_file_state_mut() { let max_offset = fs.files.len().saturating_sub(fs.view_height.saturating_sub(4)); let new_offset = (fs.table_state.offset() + 3).min(max_offset); *fs.table_state.offset_mut() = new_offset; } return true; } 
  3074	                _ => {} 
  3075	            }
  3076	        }
  3077	        _ => {} 
  3078	    }
  3079	    false
  3080	}