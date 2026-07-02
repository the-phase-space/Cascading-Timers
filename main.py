import sys
import os
import configparser
from PyQt6.QtWidgets import (QApplication, QMainWindow, QWidget, QVBoxLayout, 
                             QHBoxLayout, QLabel, QLineEdit, QPushButton, 
                             QScrollArea, QFrame, QMessageBox, QFileDialog, QCheckBox,
                             QProgressBar)
from PyQt6.QtCore import Qt, QTimer, QUrl
from PyQt6.QtMultimedia import QMediaPlayer, QAudioOutput

CONFIG_FILE = 'config.ini'

class TimerWidget(QFrame):
    def __init__(self, duration_seconds, display_mode="text", parent=None):
        super().__init__(parent)
        self.total_seconds = duration_seconds
        self.remaining_seconds = duration_seconds
        self.is_active = False
        self.is_alerting = False
        self.alert_duration_remaining = 60  # Default 60s alert duration
        self.display_mode = display_mode # "text" or "bar"
        self.parent_window = parent
        
        self.init_ui()
        self.update_display()
        
        # Style
        self.update_style()

    def update_style(self):
        self.setStyleSheet("""
            TimerWidget {
                background-color: #0d112b;
                border: 1px solid #fc035e;
                border-radius: 5px;
                margin-bottom: 5px;
            }
            QLabel {
                color: #ffffff;
                font-size: 16px;
                font-family: 'Segoe UI', Arial;
            }
            QPushButton {
                background-color: #fc035e;
                color: white;
                border: none;
                padding: 5px;
                border-radius: 3px;
                font-weight: bold;
            }
            QPushButton:hover {
                background-color: #ff337e;
            }
            QProgressBar {
                border: 1px solid #fc035e;
                border-radius: 3px;
                text-align: center;
                color: white;
                background-color: #0d112b;
            }
            QProgressBar::chunk {
                background-color: #fc035e;
            }
        """)

    def init_ui(self):
        layout = QHBoxLayout()
        self.setLayout(layout)
        
        self.lbl_time = QLabel()
        self.lbl_time.setAlignment(Qt.AlignmentFlag.AlignCenter)
        layout.addWidget(self.lbl_time)
        
        self.pbar = QProgressBar()
        self.pbar.setRange(0, int(self.total_seconds))
        self.pbar.setValue(int(self.remaining_seconds))
        self.pbar.setTextVisible(False) # Or true if we want text inside
        self.pbar.hide()
        layout.addWidget(self.pbar)
        
        self.btn_cancel = QPushButton("X")
        self.btn_cancel.setFixedWidth(30)
        self.btn_cancel.clicked.connect(self.request_delete)
        layout.addWidget(self.btn_cancel)
        
        self.btn_silence = QPushButton("SILENCE")
        self.btn_silence.clicked.connect(self.silence)
        self.btn_silence.hide() # Hidden by default
        layout.addWidget(self.btn_silence)

    def set_display_mode(self, mode):
        self.display_mode = mode
        if mode == "bar":
            self.lbl_time.hide()
            self.pbar.show()
        else:
            self.pbar.hide()
            self.lbl_time.show()
        self.update_display()

    def update_display(self):
        # Update Text
        m, s = divmod(int(self.remaining_seconds), 60)
        h, m = divmod(m, 60)
        if h > 0:
             self.lbl_time.setText(f"{h:02}:{m:02}:{s:02}")
        else:
             self.lbl_time.setText(f"{m:02}:{s:02}")
        
        # Update Bar
        if self.display_mode == "bar":
             self.pbar.setValue(int(self.remaining_seconds))
             
        if self.is_alerting:
            # Alert Style
            self.setStyleSheet("""
                TimerWidget {
                    background-color: #fc035e;
                    border: 2px solid #ffffff;
                }
                QLabel { color: white; font-weight: bold; }
                QProgressBar::chunk { background-color: white; }
            """)
        else:
            self.update_style()

    def tick(self):
        if self.is_active:
            if self.remaining_seconds > 0:
                self.remaining_seconds -= 1
                self.update_display()
                if self.remaining_seconds == 0:
                    self.start_alert()
        elif self.is_alerting:
            self.alert_duration_remaining -= 1
            if self.alert_duration_remaining <= 0:
                self.silence()

    def start_alert(self):
        self.is_active = False
        self.is_alerting = True
        self.btn_cancel.hide()
        self.btn_silence.show()
        self.update_display()
        # Parent window handles sound
        if self.parent_window:
            self.parent_window.on_timer_alert(self)

    def silence(self):
        self.is_alerting = False
        self.btn_silence.hide()
        # Spec: "A timer can simply disappear... once its alert sound is no longer playing"
        # So we delete it now.
        self.deleteLater()
        if self.parent_window:
            self.parent_window.stop_sound()
            self.parent_window.remove_timer_from_list(self)

    def request_delete(self):
        # Confirmation skipped for individual delete per spec implication of "quick", 
        # but user said "Cancel All prompts 'are you sure'". 
        # For individual, user said "individual timers can be CANCELED (upon which they disappear)".
        self.deleteLater()
        if self.parent_window:
            self.parent_window.remove_timer_from_list(self)

    def set_active(self, active):
        if not self.is_alerting and self.remaining_seconds > 0:
            self.is_active = active

class MainWindow(QMainWindow):
    def __init__(self):
        super().__init__()
        self.timers = []
        self.is_paused = True
        self.config = configparser.ConfigParser()
        self.load_config()
        
        self.init_ui()
        self.setup_sound()
        
        # App-wide heartbeat (1 second)
        self.global_timer = QTimer()
        self.global_timer.timeout.connect(self.global_tick)
        self.global_timer.start(1000)

    def load_config(self):
        if not os.path.exists(CONFIG_FILE):
            self.config['Settings'] = {
                'interval': '',
                'timer_count': '',
                'sound_file': '',
                'progress_bar_mode': 'False'
            }
            self.save_config()
        else:
            self.config.read(CONFIG_FILE)

    def save_config(self):
        with open(CONFIG_FILE, 'w') as f:
            self.config.write(f)

    def init_ui(self):
        self.setWindowTitle("Cascading Timers")
        self.setFixedWidth(400)
        self.setMinimumHeight(600)
        
        # Spec Colors
        # Background: #010421, Accent: #fc035e
        self.setStyleSheet("""
            QMainWindow { background-color: #010421; }
            QWidget { background-color: #010421; color: white; }
            QLabel { font-size: 14px; }
            QLineEdit { 
                background-color: #0d112b; 
                border: 1px solid #fc035e; 
                color: white; 
                padding: 5px; 
                border-radius: 3px;
            }
            QPushButton {
                background-color: #fc035e;
                color: white;
                border: none;
                padding: 8px;
                border-radius: 4px;
                font-weight: bold;
                font-size: 14px;
            }
            QPushButton:disabled {
                background-color: #555;
                color: #aaa;
            }
            QPushButton:hover {
                background-color: #ff337e;
            }
            QScrollArea { border: none; }
        """)

        central_widget = QWidget()
        self.setCentralWidget(central_widget)
        main_layout = QVBoxLayout(central_widget)
        main_layout.setSpacing(15)

        # --- Settings Section ---
        settings_frame = QFrame()
        settings_layout = QVBoxLayout(settings_frame)
        
        # Interval
        h_interval = QHBoxLayout()
        h_interval.addWidget(QLabel("Interval:"))
        self.input_interval = QLineEdit()
        self.input_interval.setPlaceholderText("HH:MM:SS")
        # Don't pre-fill - let user enter values
        self.input_interval.textChanged.connect(self.on_interval_changed)
        h_interval.addWidget(self.input_interval)
        settings_layout.addLayout(h_interval)

        # Count
        h_count = QHBoxLayout()
        h_count.addWidget(QLabel("Timer Count:"))
        self.input_count = QLineEdit()
        # Don't pre-fill - let user enter values
        self.input_count.textChanged.connect(self.on_count_changed)
        h_count.addWidget(self.input_count)
        settings_layout.addLayout(h_count)

        # Total Duration
        h_total = QHBoxLayout()
        h_total.addWidget(QLabel("Total Duration:"))
        self.input_duration = QLineEdit()
        self.input_duration.setPlaceholderText("HH:MM:SS")
        self.input_duration.textChanged.connect(self.on_duration_changed)
        h_total.addWidget(self.input_duration)
        settings_layout.addLayout(h_total)

        # Row 2.5: Offset
        h_offset = QHBoxLayout()
        h_offset.addWidget(QLabel("Offset (First Timer):"))
        self.input_offset = QLineEdit()
        self.input_offset.setPlaceholderText("HH:MM:SS")
        self.input_offset.textChanged.connect(self.on_offset_changed)
        h_offset.addWidget(self.input_offset)
        settings_layout.addLayout(h_offset)

        main_layout.addWidget(settings_frame)

        # Row 3: Display Mode and Time Adjust
        h_controls_extra = QHBoxLayout()
        
        self.chk_progressbar = QCheckBox("Show Progress Bars (No Text)")
        self.chk_progressbar.setStyleSheet("""
            QCheckBox { color: white; spacing: 5px; }
            QCheckBox::indicator { width: 13px; height: 13px; border: 1px solid #fc035e; }
            QCheckBox::indicator:checked { background-color: #fc035e; }
        """)
        self.chk_progressbar.setChecked(self.config['Settings'].getboolean('progress_bar_mode', False))
        self.chk_progressbar.toggled.connect(self.on_display_mode_changed)
        h_controls_extra.addWidget(self.chk_progressbar)
        
        h_controls_extra.addStretch()
        
        btn_minus = QPushButton("-15s")
        btn_minus.setFixedWidth(50)
        btn_minus.clicked.connect(lambda: self.adjust_time(-15))
        h_controls_extra.addWidget(btn_minus)
        
        btn_plus = QPushButton("+15s")
        btn_plus.setFixedWidth(50)
        btn_plus.clicked.connect(lambda: self.adjust_time(15))
        h_controls_extra.addWidget(btn_plus)
        
        main_layout.addLayout(h_controls_extra)

        # --- Controls ---
        controls_layout = QHBoxLayout()
        
        self.btn_start = QPushButton("Start All")
        self.btn_start.clicked.connect(self.start_all)
        controls_layout.addWidget(self.btn_start)
        
        self.btn_pause = QPushButton("Pause All")
        self.btn_pause.clicked.connect(self.pause_all)
        controls_layout.addWidget(self.btn_pause)
        
        self.btn_clear_all = QPushButton("Clear All")
        self.btn_clear_all.setStyleSheet("background-color: #ab0000;")
        self.btn_clear_all.clicked.connect(self.clear_all)
        controls_layout.addWidget(self.btn_clear_all)
        
        main_layout.addLayout(controls_layout)

        # --- Timers List ---
        self.scroll_area = QScrollArea()
        self.scroll_area.setWidgetResizable(True)
        self.scroll_content = QWidget()
        self.timers_layout = QVBoxLayout(self.scroll_content)
        self.timers_layout.setAlignment(Qt.AlignmentFlag.AlignTop)
        self.scroll_area.setWidget(self.scroll_content)
        
        main_layout.addWidget(self.scroll_area)
        
        # Sound Section
        sound_layout = QVBoxLayout()
        
        self.lbl_sound_name = QLabel("No sound selected")
        self.lbl_sound_name.setStyleSheet("color: #aaa; font-style: italic; font-size: 11px;")
        sound_layout.addWidget(self.lbl_sound_name)

        sound_controls = QHBoxLayout()
        
        self.btn_sound = QPushButton("Select Alert Sound...")
        self.btn_sound.clicked.connect(self.select_sound)
        self.btn_sound.setStyleSheet("background-color: #333; font-size: 11px; padding: 4px;")
        sound_controls.addWidget(self.btn_sound)
        
        self.btn_preview = QPushButton("Preview")
        self.btn_preview.clicked.connect(self.preview_sound)
        self.btn_preview.setStyleSheet("background-color: #333; font-size: 11px; padding: 4px;")
        self.btn_preview.setDisabled(True)
        sound_controls.addWidget(self.btn_preview)
        
        sound_layout.addLayout(sound_controls)
        main_layout.addLayout(sound_layout)

        # Don't populate on init - wait for user to fill fields

    def setup_sound(self):
        # Use QMediaPlayer for better compatibility (MP3 etc)
        self.player = QMediaPlayer()
        self.audio_output = QAudioOutput()
        self.player.setAudioOutput(self.audio_output)
        self.audio_output.setVolume(1.0)
        
        self.preview_timer = QTimer()
        self.preview_timer.setSingleShot(True)
        self.preview_timer.timeout.connect(self.stop_preview)
        
        # Audio timeout (4 minutes)
        self.audio_timeout_timer = QTimer()
        self.audio_timeout_timer.setSingleShot(True)
        self.audio_timeout_timer.setInterval(4 * 60 * 1000) # 4 minutes
        self.audio_timeout_timer.timeout.connect(self.stop_sound)

        sound_path = self.config['Settings'].get('sound_file', '')
        if sound_path and os.path.exists(sound_path):
            self.player.setSource(QUrl.fromLocalFile(sound_path))
            self.lbl_sound_name.setText(os.path.basename(sound_path))
            self.btn_preview.setDisabled(False)
        else:
            self.lbl_sound_name.setText("No sound selected")
            self.btn_preview.setDisabled(True)

    def select_sound(self):
        file_name, _ = QFileDialog.getOpenFileName(self, "Select Alert Sound", "", "Audio Files (*.wav *.mp3)")
        if file_name:
            self.config['Settings']['sound_file'] = file_name
            self.save_config()
            self.player.setSource(QUrl.fromLocalFile(file_name))
            self.lbl_sound_name.setText(os.path.basename(file_name))
            self.btn_preview.setDisabled(False)

    def preview_sound(self):
        if self.player.source().isValid():
            self.player.stop()
            self.player.setLoops(1) # Play once
            self.player.play()
            self.preview_timer.start(2000)

    def stop_preview(self):
        self.player.stop()

    def play_sound(self):
        # Called when alert should be playing
        if self.player.source().isValid():
            if self.player.playbackState() != QMediaPlayer.PlaybackState.PlayingState:
                self.player.setLoops(QMediaPlayer.Loops.Infinite)
                self.player.play()
                self.audio_timeout_timer.start() # Start 4 min timeout
        else:
            QApplication.beep()

    def stop_sound(self):
        if self.player.playbackState() == QMediaPlayer.PlaybackState.PlayingState:
            self.player.stop()
        self.audio_timeout_timer.stop()

    def on_timer_alert(self, timer_widget):
        # Called by TimerWidget when it STARTS alerting
        # Reset sound logic (restart sound if stopped, or refresh timeout)
        self.stop_sound()
        self.play_sound()

    def parse_time_str(self, time_str):
        total = 0
        time_str = time_str.lower().strip()
        if not time_str: return 0
        
        # Check for HH:MM:SS format
        if ':' in time_str:
            parts = time_str.split(':')
            parts.reverse() # s, m, h
            try:
                if len(parts) >= 1: total += int(parts[0]) # Seconds
                if len(parts) >= 2: total += int(parts[1]) * 60 # Minutes
                if len(parts) >= 3: total += int(parts[2]) * 3600 # Hours
            except ValueError:
                return 0
            return total
        
        parts = time_str.split()
        
        for part in parts:
            if part.endswith('h'):
                total += int(float(part[:-1]) * 3600)
            elif part.endswith('m'):
                total += int(float(part[:-1]) * 60)
            elif part.endswith('s'):
                total += int(float(part[:-1]))
            elif part.isdigit():
                 total += int(part)
        return total

    def get_friendly_time(self, seconds):
        if seconds % 3600 == 0:
            return f"{seconds // 3600}h"
        if seconds % 60 == 0:
            return f"{seconds // 60}m"
        return f"{seconds}s"

    # --- Logic ---
    
    def adjust_time(self, delta_seconds):
        # Subtract/Add time to all NONZERO timers
        for t in self.timers:
            if t.remaining_seconds > 0:
                new_time = t.remaining_seconds + delta_seconds
                if new_time < 0:
                    new_time = 0
                t.remaining_seconds = new_time
                t.update_display()
                # If it hit 0, it will trigger alert on next global tick?
                # or we can force check?
                # tick() logic: if rem > 0 -> rem -= 1 ... if rem == 0 -> alert.
                # If we set rem=0 here, the next tick() will see 0.
                # But modify: tick check is "if remaining > 0: ... if remaining == 0"
                # If we set to 0 here, next tick sees 0. It won't decrement. 
                # It won't enter "remaining > 0" block.
                # It won't trigger start_alert().
                # Fix: If we set to 0, call start_alert?
                if new_time == 0:
                    t.start_alert()

    def on_settings_change(self):
        self.update_field_states()

    def on_interval_changed(self):
        self.update_field_states()

    def on_count_changed(self):
        self.update_field_states()

    def on_duration_changed(self):
        self.update_field_states()

    def on_offset_changed(self):
        """When offset changes, recalculate the disabled field"""
        # Determine which field is currently disabled and recalculate it
        if not self.input_interval.isEnabled():
            self.calculate_interval()
        elif not self.input_count.isEnabled():
            self.calculate_count()
        elif not self.input_duration.isEnabled():
            self.calculate_duration()

        # Rebuild timers if we have enough data
        interval_filled = bool(self.input_interval.text().strip())
        count_filled = bool(self.input_count.text().strip())
        duration_filled = bool(self.input_duration.text().strip())
        filled_count = sum([interval_filled, count_filled, duration_filled])

        if filled_count >= 2:
            self.process_inputs_and_populate_preview()

    def enable_field(self, field):
        field.setDisabled(False)
        field.setStyleSheet("background-color: #0d112b; color: white; border: 1px solid #fc035e; padding: 5px; border-radius: 3px;")

    def disable_field(self, field):
        field.setDisabled(True)
        field.setStyleSheet("background-color: #333; color: #555; border: 1px solid #555; padding: 5px; border-radius: 3px;")

    def calculate_interval(self):
        """Calculate interval from count and duration"""
        try:
            count_text = self.input_count.text().strip()
            duration_text = self.input_duration.text().strip()

            if not count_text or not duration_text:
                return

            count = int(count_text)
            total_duration = self.parse_time_str(duration_text)

            if count <= 1 or total_duration <= 0:
                return

            offset_s = self.parse_time_str(self.input_offset.text().strip())

            # total_duration = offset + (count - 1) * interval
            # interval = (total_duration - offset) / (count - 1)
            if offset_s > 0:
                interval = (total_duration - offset_s) / (count - 1)
            else:
                interval = total_duration / count

            if interval > 0:
                self.input_interval.blockSignals(True)
                self.input_interval.setText(self.get_friendly_time(int(interval)))
                self.input_interval.blockSignals(False)
        except (ValueError, ZeroDivisionError, AttributeError):
            pass

    def calculate_count(self):
        """Calculate count from interval and duration"""
        try:
            interval_text = self.input_interval.text().strip()
            duration_text = self.input_duration.text().strip()

            if not interval_text or not duration_text:
                return

            interval = self.parse_time_str(interval_text)
            total_duration = self.parse_time_str(duration_text)

            if interval <= 0 or total_duration <= 0:
                return

            offset_s = self.parse_time_str(self.input_offset.text().strip())

            # total_duration = offset + (count - 1) * interval
            # count = ((total_duration - offset) / interval) + 1
            import math
            if offset_s > 0:
                if total_duration < offset_s:
                    count = 0
                else:
                    count = math.floor((total_duration - offset_s) / interval) + 1
            else:
                count = math.floor(total_duration / interval)

            if count > 0:
                self.input_count.blockSignals(True)
                self.input_count.setText(str(count))
                self.input_count.blockSignals(False)
        except (ValueError, ZeroDivisionError, AttributeError):
            pass

    def calculate_duration(self):
        """Calculate duration from interval and count"""
        try:
            interval_text = self.input_interval.text().strip()
            count_text = self.input_count.text().strip()

            if not interval_text or not count_text:
                return

            interval = self.parse_time_str(interval_text)
            count = int(count_text)

            if interval <= 0 or count <= 0:
                return

            offset_s = self.parse_time_str(self.input_offset.text().strip())

            # total_duration = offset + (count - 1) * interval
            if offset_s > 0:
                total_duration = offset_s + (count - 1) * interval
            else:
                total_duration = count * interval

            self.input_duration.blockSignals(True)
            self.input_duration.setText(self.get_friendly_time(int(total_duration)))
            self.input_duration.blockSignals(False)
        except (ValueError, AttributeError):
            pass

    def is_valid_filled(self, field, is_count_field=False):
        """Check if a field has a valid value (not just any text)"""
        text = field.text().strip()
        if not text:
            return False

        # Ignore placeholder-like text
        if text.upper() in ["HH:MM:SS", "H:M:S"]:
            return False

        try:
            if is_count_field:
                return int(text) > 0
            else:
                return self.parse_time_str(text) > 0
        except (ValueError, AttributeError):
            return False

    def update_field_states(self):
        """Update which fields are enabled/disabled based on which are filled"""
        interval_filled = self.is_valid_filled(self.input_interval)
        count_filled = self.is_valid_filled(self.input_count, is_count_field=True)
        duration_filled = self.is_valid_filled(self.input_duration)

        filled_count = sum([interval_filled, count_filled, duration_filled])

        if filled_count >= 2:
            # Determine which field to disable and calculate
            if not interval_filled:
                self.disable_field(self.input_interval)
                self.calculate_interval()
                self.enable_field(self.input_count)
                self.enable_field(self.input_duration)
            elif not count_filled:
                self.disable_field(self.input_count)
                self.calculate_count()
                self.enable_field(self.input_interval)
                self.enable_field(self.input_duration)
            elif not duration_filled:
                self.disable_field(self.input_duration)
                self.calculate_duration()
                self.enable_field(self.input_interval)
                self.enable_field(self.input_count)
            else:
                # All three filled - prefer interval + count, recalculate duration
                self.disable_field(self.input_duration)
                self.calculate_duration()
                self.enable_field(self.input_interval)
                self.enable_field(self.input_count)

            # Rebuild timers with the calculated values
            self.process_inputs_and_populate_preview()
        else:
            # Less than 2 filled - enable all, don't rebuild
            self.enable_field(self.input_interval)
            self.enable_field(self.input_count)
            self.enable_field(self.input_duration)

    def on_display_mode_changed(self):
        mode = "bar" if self.chk_progressbar.isChecked() else "text"
        self.config['Settings']['progress_bar_mode'] = str(self.chk_progressbar.isChecked())
        self.save_config()
        
        # Update existing
        for t in self.timers:
            t.set_display_mode(mode)

    def process_inputs_and_populate_preview(self):
        if any(t.is_active for t in self.timers) or any(t.remaining_seconds < t.total_seconds for t in self.timers):
            pass
        else:
            self.rebuild_timers()

    def rebuild_timers(self):
        # 1. Clear existing
        while self.timers_layout.count():
            item = self.timers_layout.takeAt(0)
            if item.widget():
                item.widget().deleteLater()
        self.timers = []

        # 2. Parse inputs
        try:
            interval_s = self.parse_time_str(self.input_interval.text())
            if interval_s <= 0: return

            count = 0
            if self.input_count.isEnabled() and self.input_count.text().strip():
                count = int(self.input_count.text())
            elif self.input_duration.isEnabled() and self.input_duration.text().strip():
                total_s = self.parse_time_str(self.input_duration.text())
                if total_s > 0:
                    import math
                    # If offset is present, calculation is tricky.
                    # Total Duration implies last timer ends at Total Duration.
                    # If offset present: T_last = offset + (count-1)*interval.
                    # T_last <= total_s.
                    # offset + (count-1)*interval <= total_s
                    # (count-1)*interval <= total_s - offset
                    # count-1 <= (total_s - offset) / interval
                    # count <= ((total_s - offset) / interval) + 1
                    
                    offset_s = self.parse_time_str(self.input_offset.text())
                    
                    if offset_s > 0:
                         if total_s < offset_s:
                             count = 0
                         else:
                             count = math.floor(((total_s - offset_s) / interval_s)) + 1
                    else:
                        count = math.floor(total_s / interval_s)

            if count <= 0: return
            
            # 2.5 Cap at 20 per request
            if count > 20: 
                count = 20

            # 3. Create Widgets
            mode = "bar" if self.chk_progressbar.isChecked() else "text"
            
            offset_s = self.parse_time_str(self.input_offset.text())
            start_base = offset_s if offset_s > 0 else interval_s
            
            for i in range(count):
                # i=0 -> start_base
                # i=1 -> start_base + interval
                # ...
                duration = start_base + (i * interval_s)
                
                t_widget = TimerWidget(duration, display_mode=mode, parent=self)
                t_widget.parent_window = self
                self.timers.append(t_widget)
                self.timers_layout.addWidget(t_widget)

            # Save valid interval to config (only if actually valid)
            if interval_s > 0:
                self.config['Settings']['interval'] = str(interval_s)
                if count > 0:
                    self.config['Settings']['timer_count'] = str(count)
                self.save_config()

        except ValueError:
            pass

    def start_all(self):
        if not self.timers:
            self.rebuild_timers()
        
        for t in self.timers:
            t.set_active(True)
        self.is_paused = False

    def pause_all(self):
        self.is_paused = True
        for t in self.timers:
            t.set_active(False)
        self.stop_sound()

    def clear_all(self):
        reply = QMessageBox.question(self, 'Confirm Clear', 
                                     "Are you sure you want to clear all timers?",
                                     QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No, 
                                     QMessageBox.StandardButton.No)
        if reply == QMessageBox.StandardButton.Yes:
            self.pause_all()
            # Clearing inputs triggers signals which calls rebuild_timers -> clear list
            self.input_count.clear()
            self.input_duration.clear()
            self.input_offset.clear() # Clear offset too
            # Ensure they are re-enabled
            self.input_count.setDisabled(False)
            self.input_duration.setDisabled(False)
            self.input_count.setStyleSheet("background-color: #0d112b; color: white; border: 1px solid #fc035e;")
            self.input_duration.setStyleSheet("background-color: #0d112b; color: white; border: 1px solid #fc035e;")
            self.rebuild_timers() # Final cleanup just in case

    def remove_timer_from_list(self, timer_widget):
        if timer_widget in self.timers:
            self.timers.remove(timer_widget)

    def global_tick(self):
        # Check alerts logic
        any_alerting = False
        alerting_timer = None
        
        for t in self.timers:
            t.tick() 
            if t.is_alerting:
                any_alerting = True
                alerting_timer = t

        # Sound logic handled via signals now for restarts
        # But we still need to ensure if NO ONE is alerting, sound stops.
        if not any_alerting:
             self.stop_sound()

def main():
    app = QApplication(sys.argv)
    window = MainWindow()
    window.show()
    sys.exit(app.exec())

if __name__ == '__main__':
    main()
