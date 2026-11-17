using System;
using System.Collections.Generic;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Media;
using System.Windows.Threading;
using MultiZaloDesktop.Core;

namespace MultiZaloDesktop.UI
{
    public sealed class MainWindow : Window
    {
        private readonly InstanceManager _manager;
        private readonly DispatcherTimer _refreshTimer;

        private TextBlock _installStatusText;
        private TextBlock _primaryStatusText;
        private TextBlock _summaryCountText;
        private TextBlock _footerPathText;
        private StackPanel _profilesPanel;
        private TextBlock _emptyStateText;

        private static readonly BrushConverter BrushConv = new BrushConverter();

        public MainWindow(InstanceManager manager)
        {
            _manager = manager;

            Title = "Multi-Zalo Desktop";
            Width = 860;
            Height = 640;
            MinWidth = 700;
            MinHeight = 480;
            WindowStartupLocation = WindowStartupLocation.CenterScreen;
            Background = GetBrush("#F5F7FA");
            FontFamily = new FontFamily("Segoe UI");

            BuildLayout();

            _refreshTimer = new DispatcherTimer();
            _refreshTimer.Interval = TimeSpan.FromSeconds(2);
            _refreshTimer.Tick += delegate { RefreshData(false); };

            Loaded += delegate
            {
                Activate();
                RefreshData(true);
                _refreshTimer.Start();
            };

            Closed += delegate
            {
                _refreshTimer.Stop();
            };
        }

        private static Brush GetBrush(string hex)
        {
            return (Brush)BrushConv.ConvertFromString(hex);
        }

        private void BuildLayout()
        {
            Grid root = new Grid();
            root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
            root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
            root.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });
            root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });

            // 1. Header Bar
            Border headerBorder = new Border();
            headerBorder.Background = GetBrush("#FFFFFF");
            headerBorder.BorderBrush = GetBrush("#E2E8F0");
            headerBorder.BorderThickness = new Thickness(0, 0, 0, 1);
            headerBorder.Padding = new Thickness(24, 16, 24, 16);

            Grid headerGrid = new Grid();
            headerGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
            headerGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });

            StackPanel titleStack = new StackPanel();
            TextBlock titleText = new TextBlock();
            titleText.Text = "Multi-Zalo Desktop";
            titleText.FontSize = 20;
            titleText.FontWeight = FontWeights.SemiBold;
            titleText.Foreground = GetBrush("#0F172A");

            _installStatusText = new TextBlock();
            _installStatusText.Text = "Dang kiem tra ban cai dat Zalo Desktop chinh thuc...";
            _installStatusText.FontSize = 12;
            _installStatusText.Foreground = GetBrush("#475569");
            _installStatusText.Margin = new Thickness(0, 4, 0, 0);

            titleStack.Children.Add(titleText);
            titleStack.Children.Add(_installStatusText);
            Grid.SetColumn(titleStack, 0);

            StackPanel actionBar = new StackPanel();
            actionBar.Orientation = Orientation.Horizontal;
            actionBar.VerticalAlignment = VerticalAlignment.Center;

            Button refreshBtn = CreateButton("Lam moi", false);
            refreshBtn.Margin = new Thickness(0, 0, 10, 0);
            refreshBtn.Click += delegate { RefreshData(true); };

            Button addBtn = CreateButton("+ Them tai khoan", true);
            addBtn.Click += delegate { OnAddProfileClicked(); };

            actionBar.Children.Add(refreshBtn);
            actionBar.Children.Add(addBtn);
            Grid.SetColumn(actionBar, 1);

            headerGrid.Children.Add(titleStack);
            headerGrid.Children.Add(actionBar);
            headerBorder.Child = headerGrid;
            Grid.SetRow(headerBorder, 0);
            root.Children.Add(headerBorder);

            // 2. Primary Zalo Protection Info Strip
            Border infoStrip = new Border();
            infoStrip.Background = GetBrush("#EFF6FF");
            infoStrip.BorderBrush = GetBrush("#BFDBFE");
            infoStrip.BorderThickness = new Thickness(0, 0, 0, 1);
            infoStrip.Padding = new Thickness(24, 10, 24, 10);

            Grid stripGrid = new Grid();
            stripGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
            stripGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });

            _primaryStatusText = new TextBlock();
            _primaryStatusText.FontSize = 12;
            _primaryStatusText.Foreground = GetBrush("#1E3A8A");
            _primaryStatusText.VerticalAlignment = VerticalAlignment.Center;
            Grid.SetColumn(_primaryStatusText, 0);

            _summaryCountText = new TextBlock();
            _summaryCountText.FontSize = 12;
            _summaryCountText.FontWeight = FontWeights.SemiBold;
            _summaryCountText.Foreground = GetBrush("#1E40AF");
            _summaryCountText.VerticalAlignment = VerticalAlignment.Center;
            Grid.SetColumn(_summaryCountText, 1);

            stripGrid.Children.Add(_primaryStatusText);
            stripGrid.Children.Add(_summaryCountText);
            infoStrip.Child = stripGrid;
            Grid.SetRow(infoStrip, 1);
            root.Children.Add(infoStrip);

            // 3. Scrollable Profile List
            ScrollViewer scroll = new ScrollViewer();
            scroll.VerticalScrollBarVisibility = ScrollBarVisibility.Auto;
            scroll.Padding = new Thickness(24, 18, 24, 18);

            StackPanel contentContainer = new StackPanel();

            _emptyStateText = new TextBlock();
            _emptyStateText.Text = "Chua co profile nao. Nhan '+ Them tai khoan' de tao moi truong Zalo Desktop doc lap.";
            _emptyStateText.FontSize = 13;
            _emptyStateText.Foreground = GetBrush("#64748B");
            _emptyStateText.HorizontalAlignment = HorizontalAlignment.Center;
            _emptyStateText.Margin = new Thickness(0, 48, 0, 0);
            _emptyStateText.Visibility = Visibility.Collapsed;

            _profilesPanel = new StackPanel();
            contentContainer.Children.Add(_emptyStateText);
            contentContainer.Children.Add(_profilesPanel);

            scroll.Content = contentContainer;
            Grid.SetRow(scroll, 2);
            root.Children.Add(scroll);

            // 4. Footer Status Bar
            Border footer = new Border();
            footer.Background = GetBrush("#FFFFFF");
            footer.BorderBrush = GetBrush("#E2E8F0");
            footer.BorderThickness = new Thickness(0, 1, 0, 0);
            footer.Padding = new Thickness(24, 10, 24, 10);

            _footerPathText = new TextBlock();
            _footerPathText.FontSize = 11.5;
            _footerPathText.Foreground = GetBrush("#64748B");
            footer.Child = _footerPathText;
            Grid.SetRow(footer, 3);
            root.Children.Add(footer);

            Content = root;
        }

        private void RefreshData(bool includeDiskSizes)
        {
            try
            {
                StatusSnapshot snap = _manager.CaptureStatus(includeDiskSizes);

                if (snap.Installation != null && snap.Installation.IsValid)
                {
                    _installStatusText.Text = string.Format(
                        "Zalo Desktop chinh thuc v{0}  |  {1}  |  {2}",
                        snap.Installation.Version,
                        snap.Installation.AuthenticodeStatus,
                        snap.Installation.ElectronExePath);
                    _installStatusText.Foreground = GetBrush("#475569");
                }
                else
                {
                    _installStatusText.Text = snap.Installation != null ? snap.Installation.Error : "Khong tim thay Zalo Desktop";
                    _installStatusText.Foreground = GetBrush("#DC2626");
                }

                if (snap.PrimaryZalo != null)
                {
                    if (snap.PrimaryZalo.IsRunning)
                    {
                        double mb = snap.PrimaryZalo.TotalWorkingSetBytes / (1024.0 * 1024.0);
                        _primaryStatusText.Text = string.Format(
                            "Zalo mac dinh cua he thong: Dang chay (PID {0}, {1} tien trinh, {2:F0} MB) - Duoc tach biet va bao ve",
                            string.Join(",", snap.PrimaryZalo.LockingPids),
                            snap.PrimaryZalo.ProcessCount,
                            mb);
                    }
                    else
                    {
                        _primaryStatusText.Text = "Zalo mac dinh cua he thong: Da dung - Profile goc tai %APPDATA%\\ZaloData khong bi anh huong";
                    }
                }

                int runningCount = 0;
                for (int i = 0; i < snap.Profiles.Count; i++)
                {
                    if (snap.Profiles[i].IsRunning)
                    {
                        runningCount++;
                    }
                }

                _summaryCountText.Text = string.Format("{0} profile  |  {1} dang chay", snap.Profiles.Count, runningCount);
                _footerPathText.Text = "Thu muc luu tru co lap: " + snap.StorageRoot + "  |  Khong sua doi Zalo.exe, khong luu mat khau/token.";

                RenderProfiles(snap.Profiles);
            }
            catch (Exception ex)
            {
                _footerPathText.Text = "Loi cap nhat trang thai: " + ex.Message;
            }
        }

        private void RenderProfiles(List<ProfileRuntimeState> profiles)
        {
            _profilesPanel.Children.Clear();
            if (profiles == null || profiles.Count == 0)
            {
                _emptyStateText.Visibility = Visibility.Visible;
                return;
            }
            _emptyStateText.Visibility = Visibility.Collapsed;

            for (int i = 0; i < profiles.Count; i++)
            {
                _profilesPanel.Children.Add(BuildProfileCard(profiles[i]));
            }
        }

        private Border BuildProfileCard(ProfileRuntimeState state)
        {
            Border card = new Border();
            card.Background = GetBrush("#FFFFFF");
            card.BorderBrush = state.IsRunning ? GetBrush("#93C5FD") : GetBrush("#E2E8F0");
            card.BorderThickness = new Thickness(1);
            card.CornerRadius = new CornerRadius(8);
            card.Padding = new Thickness(16, 14, 16, 14);
            card.Margin = new Thickness(0, 0, 0, 10);

            Grid grid = new Grid();
            grid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
            grid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });

            // Left info block
            StackPanel left = new StackPanel();
            left.VerticalAlignment = VerticalAlignment.Center;

            StackPanel topRow = new StackPanel();
            topRow.Orientation = Orientation.Horizontal;

            TextBlock nameBlock = new TextBlock();
            nameBlock.Text = state.ProfileName;
            nameBlock.FontSize = 15;
            nameBlock.FontWeight = FontWeights.SemiBold;
            nameBlock.Foreground = GetBrush("#0F172A");
            nameBlock.VerticalAlignment = VerticalAlignment.Center;

            Border statusBadge = new Border();
            statusBadge.CornerRadius = new CornerRadius(10);
            statusBadge.Padding = new Thickness(8, 2, 8, 2);
            statusBadge.Margin = new Thickness(10, 0, 0, 0);
            statusBadge.VerticalAlignment = VerticalAlignment.Center;
            statusBadge.Background = state.IsRunning ? GetBrush("#DCFCE7") : GetBrush("#F1F5F9");

            TextBlock statusLabel = new TextBlock();
            statusLabel.Text = state.IsRunning ? "Dang chay" : "Da dung";
            statusLabel.FontSize = 11;
            statusLabel.FontWeight = FontWeights.SemiBold;
            statusLabel.Foreground = state.IsRunning ? GetBrush("#166534") : GetBrush("#475569");
            statusBadge.Child = statusLabel;

            TextBlock idLabel = new TextBlock();
            idLabel.Text = "#" + state.ProfileId;
            idLabel.FontSize = 11;
            idLabel.Foreground = GetBrush("#94A3B8");
            idLabel.VerticalAlignment = VerticalAlignment.Center;
            idLabel.Margin = new Thickness(8, 0, 0, 0);

            topRow.Children.Add(nameBlock);
            topRow.Children.Add(statusBadge);
            topRow.Children.Add(idLabel);

            TextBlock metaLine = new TextBlock();
            metaLine.FontSize = 12;
            metaLine.Foreground = GetBrush("#64748B");
            metaLine.Margin = new Thickness(0, 6, 0, 0);

            if (state.IsRunning)
            {
                double ramMb = state.TotalWorkingSetBytes / (1024.0 * 1024.0);
                metaLine.Text = string.Format(
                    "PID chinh: {0}  |  {1} tien trinh  |  {2:F1} MB RAM  |  {3}",
                    state.MainPid,
                    state.ProcessCount,
                    ramMb,
                    string.IsNullOrEmpty(state.WindowTitle) ? "Cua so Zalo" : state.WindowTitle);
            }
            else
            {
                metaLine.Text = "Duong dan du lieu: " + state.ZaloDataPath;
            }

            left.Children.Add(topRow);
            left.Children.Add(metaLine);
            Grid.SetColumn(left, 0);

            // Right actions
            StackPanel actions = new StackPanel();
            actions.Orientation = Orientation.Horizontal;
            actions.VerticalAlignment = VerticalAlignment.Center;
            actions.Margin = new Thickness(16, 0, 0, 0);

            string profileId = state.ProfileId;
            string profileName = state.ProfileName;

            Button launchBtn = CreateButton(state.IsRunning ? "Dua len truoc" : "Mo Zalo", true);
            launchBtn.Margin = new Thickness(0, 0, 8, 0);
            launchBtn.Click += delegate { OnLaunchOrFocusClicked(profileId); };
            actions.Children.Add(launchBtn);

            if (state.IsRunning)
            {
                Button stopBtn = CreateButton("Dong phien", false);
                stopBtn.Margin = new Thickness(0, 0, 8, 0);
                stopBtn.Click += delegate { OnStopClicked(profileId, profileName); };
                actions.Children.Add(stopBtn);
            }

            Button renameBtn = CreateButton("Doi ten", false);
            renameBtn.Margin = new Thickness(0, 0, 8, 0);
            renameBtn.Click += delegate { OnRenameClicked(profileId, profileName); };
            actions.Children.Add(renameBtn);

            Button folderBtn = CreateButton("Thu muc", false);
            folderBtn.Margin = new Thickness(0, 0, 8, 0);
            folderBtn.Click += delegate { _manager.OpenProfileFolder(profileId); };
            actions.Children.Add(folderBtn);

            Button deleteBtn = CreateDangerButton("Xoa");
            deleteBtn.IsEnabled = !state.IsRunning;
            if (state.IsRunning)
            {
                deleteBtn.ToolTip = "Hay dong phien truoc khi xoa profile.";
            }
            deleteBtn.Click += delegate { OnDeleteClicked(profileId, profileName); };
            actions.Children.Add(deleteBtn);

            Grid.SetColumn(actions, 1);

            grid.Children.Add(left);
            grid.Children.Add(actions);
            card.Child = grid;
            return card;
        }

        private Button CreateButton(string text, bool primary)
        {
            Button btn = new Button();
            btn.Content = text;
            btn.Padding = new Thickness(12, 6, 12, 6);
            btn.FontSize = 12;
            btn.FontWeight = FontWeights.Medium;
            btn.Cursor = Cursors.Hand;
            btn.BorderThickness = new Thickness(1);

            if (primary)
            {
                btn.Background = GetBrush("#0068FF");
                btn.Foreground = GetBrush("#FFFFFF");
                btn.BorderBrush = GetBrush("#005AE0");
            }
            else
            {
                btn.Background = GetBrush("#FFFFFF");
                btn.Foreground = GetBrush("#334155");
                btn.BorderBrush = GetBrush("#CBD5E1");
            }
            return btn;
        }

        private Button CreateDangerButton(string text)
        {
            Button btn = new Button();
            btn.Content = text;
            btn.Padding = new Thickness(10, 6, 10, 6);
            btn.FontSize = 12;
            btn.FontWeight = FontWeights.Medium;
            btn.Cursor = Cursors.Hand;
            btn.Background = GetBrush("#FEF2F2");
            btn.Foreground = GetBrush("#B91C1C");
            btn.BorderBrush = GetBrush("#FECACA");
            btn.BorderThickness = new Thickness(1);
            return btn;
        }

        private void OnAddProfileClicked()
        {
            string input = PromptTextDialog("Them tai khoan moi", "Nhap ten goi nho cho profile (vi du: Cong viec, Ca nhan):", "Tai khoan moi");
            if (string.IsNullOrEmpty(input))
            {
                return;
            }
            OperationResult res = _manager.CreateProfile(input, string.Empty);
            if (!res.Success)
            {
                MessageBox.Show(this, res.Message, "Multi-Zalo Desktop", MessageBoxButton.OK, MessageBoxImage.Warning);
                return;
            }
            RefreshData(true);
        }

        private void OnRenameClicked(string profileId, string currentName)
        {
            string input = PromptTextDialog("Doi ten profile", "Nhap ten moi cho profile:", currentName);
            if (string.IsNullOrEmpty(input) || input == currentName)
            {
                return;
            }
            OperationResult res = _manager.RenameProfile(profileId, input);
            if (!res.Success)
            {
                MessageBox.Show(this, res.Message, "Multi-Zalo Desktop", MessageBoxButton.OK, MessageBoxImage.Warning);
                return;
            }
            RefreshData(false);
        }

        private void OnLaunchOrFocusClicked(string profileId)
        {
            OperationResult res = _manager.LaunchOrFocusProfile(profileId);
            if (!res.Success)
            {
                MessageBox.Show(this, res.Message, "Multi-Zalo Desktop", MessageBoxButton.OK, MessageBoxImage.Error);
            }
            RefreshData(false);
        }

        private void OnStopClicked(string profileId, string profileName)
        {
            MessageBoxResult confirm = MessageBox.Show(
                this,
                "Ban co muon dong phien Zalo Desktop cua profile '" + profileName + "'?\nCac phien Zalo khac se khong bi anh huong.",
                "Xac nhan dong phien",
                MessageBoxButton.YesNo,
                MessageBoxImage.Question);

            if (confirm != MessageBoxResult.Yes)
            {
                return;
            }

            OperationResult res = _manager.StopProfile(profileId);
            if (!res.Success)
            {
                MessageBox.Show(this, res.Message, "Multi-Zalo Desktop", MessageBoxButton.OK, MessageBoxImage.Error);
            }
            RefreshData(false);
        }

        private void OnDeleteClicked(string profileId, string profileName)
        {
            MessageBoxResult confirm = MessageBox.Show(
                this,
                "CANH BAO: Xoa profile '" + profileName + "' se xoa toan bo du lieu dang nhap va cache cuc bo trong vung co lap cua profile nay.\n\nZalo chinh cua may se khong bi anh huong. Ban co chac chan?",
                "Xac nhan xoa profile",
                MessageBoxButton.YesNo,
                MessageBoxImage.Warning);

            if (confirm != MessageBoxResult.Yes)
            {
                return;
            }

            OperationResult res = _manager.DeleteProfile(profileId);
            if (!res.Success)
            {
                MessageBox.Show(this, res.Message, "Multi-Zalo Desktop", MessageBoxButton.OK, MessageBoxImage.Error);
                return;
            }
            RefreshData(true);
        }

        private string PromptTextDialog(string title, string prompt, string defaultValue)
        {
            Window dlg = new Window();
            dlg.Title = title;
            dlg.Width = 420;
            dlg.SizeToContent = SizeToContent.Height;
            dlg.ResizeMode = ResizeMode.NoResize;
            dlg.WindowStartupLocation = WindowStartupLocation.CenterOwner;
            dlg.Owner = this;
            dlg.Background = GetBrush("#FFFFFF");
            dlg.FontFamily = new FontFamily("Segoe UI");

            StackPanel stack = new StackPanel();
            stack.Margin = new Thickness(20);

            TextBlock lbl = new TextBlock();
            lbl.Text = prompt;
            lbl.FontSize = 13;
            lbl.Foreground = GetBrush("#334155");
            lbl.TextWrapping = TextWrapping.Wrap;
            lbl.Margin = new Thickness(0, 0, 0, 10);

            TextBox tb = new TextBox();
            tb.Text = defaultValue ?? string.Empty;
            tb.Padding = new Thickness(8, 6, 8, 6);
            tb.FontSize = 13;
            tb.BorderBrush = GetBrush("#CBD5E1");
            tb.BorderThickness = new Thickness(1);
            tb.SelectAll();

            StackPanel btnRow = new StackPanel();
            btnRow.Orientation = Orientation.Horizontal;
            btnRow.HorizontalAlignment = HorizontalAlignment.Right;
            btnRow.Margin = new Thickness(0, 16, 0, 0);

            string acceptedText = null;

            Button cancelBtn = CreateButton("Huy", false);
            cancelBtn.Margin = new Thickness(0, 0, 8, 0);
            cancelBtn.IsCancel = true;
            cancelBtn.Click += delegate { dlg.DialogResult = false; };

            Button okBtn = CreateButton("Xac nhan", true);
            okBtn.IsDefault = true;
            okBtn.Click += delegate
            {
                acceptedText = (tb.Text ?? string.Empty).Trim();
                dlg.DialogResult = true;
            };

            btnRow.Children.Add(cancelBtn);
            btnRow.Children.Add(okBtn);

            stack.Children.Add(lbl);
            stack.Children.Add(tb);
            stack.Children.Add(btnRow);
            dlg.Content = stack;

            tb.Focus();
            bool? result = dlg.ShowDialog();
            return (result == true) ? acceptedText : null;
        }
    }
}
