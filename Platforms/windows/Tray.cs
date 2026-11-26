using System.Runtime.InteropServices;
using System.Text;

namespace Radio;

/// <summary>
/// The notification-area icon. WinUI 3 has no tray API of its own, and Windows
/// 11's own is still <c>Shell_NotifyIcon</c>, so this is the one place where the
/// app talks to Win32 directly.
/// </summary>
internal sealed class Tray : IDisposable
{
    private const int WM_APP = 0x8000;
    private const int CallbackMessage = WM_APP + 21;
    private const uint NIM_ADD = 0;
    private const uint NIM_MODIFY = 1;
    private const uint NIM_DELETE = 2;
    private const uint NIF_MESSAGE = 0x01;
    private const uint NIF_ICON = 0x02;
    private const uint NIF_TIP = 0x04;
    private const uint NIF_INFO = 0x10;
    private const uint NIIF_INFO = 0x1;
    private const uint IMAGE_ICON = 1;
    private const uint LR_LOADFROMFILE = 0x0010;
    private const int SM_CXSMICON = 49;
    private const int SM_CYSMICON = 50;
    private const uint MF_STRING = 0;
    private const uint TPM_LEFTALIGN = 0;
    private const uint TPM_BOTTOMALIGN = 0x0020;
    private const uint TPM_RIGHTBUTTON = 0x0002;
    private const uint TPM_RETURNCMD = 0x0100;
    private const int WM_LBUTTONUP = 0x0202;
    private const int WM_RBUTTONUP = 0x0205;
    private const int NIN_BALLOONUSERCLICK = 0x0405;

    /// <summary>
    /// A hidden top-level window, not a message-only one: Explorer announces
    /// "TaskbarCreated" with a broadcast, and broadcasts are not delivered to
    /// message-only windows, so the icon would never come back after a restart.
    /// </summary>
    private static readonly IntPtr NoParent = IntPtr.Zero;
    /// <summary>
    /// The icon is identified by this window plus <see cref="IconId"/>, not by a
    /// GUID. A GUID would be a new icon to the shell: a fresh registry entry,
    /// back in the overflow, and whatever the user chose about it is lost. This
    /// identity is the one the shell already knows.
    /// </summary>
    private const uint IconId = 1;

    private readonly WndProc proc;
    private IntPtr window;
    private IntPtr icon;
    private uint taskbarCreated;
    private Action? quit;

    public Tray() => proc = WindowProc;

    public Action? LeftClick { get; set; }

    public void Install(Action onQuit)
    {
        quit = onQuit;
        taskbarCreated = RegisterWindowMessageW("TaskbarCreated");

        var className = "RadioTrayWindow";
        var instance = GetModuleHandleW(null);
        var windowClass = new WNDCLASSW
        {
            lpfnWndProc = proc,
            hInstance = instance,
            lpszClassName = className,
        };
        RegisterClassW(ref windowClass);

        window = CreateWindowExW(0, className, "Radio", 0, 0, 0, 0, 0, NoParent, IntPtr.Zero, instance, IntPtr.Zero);
        if (window == IntPtr.Zero)
        {
            return;
        }

        icon = LoadImageW(IntPtr.Zero, IconPath(), IMAGE_ICON,
            GetSystemMetrics(SM_CXSMICON), GetSystemMetrics(SM_CYSMICON), LR_LOADFROMFILE);

        var data = Data();
        Shell_NotifyIconW(NIM_ADD, ref data);
    }

    public void ShowMenu()
    {
        if (window == IntPtr.Zero)
        {
            return;
        }

        var menu = CreatePopupMenu();
        AppendMenuW(menu, MF_STRING, 1, "Quit Radio");
        GetCursorPos(out var point);

        // Without this the menu stays up after a click elsewhere.
        SetForegroundWindow(window);
        var chosen = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN | TPM_LEFTALIGN,
            point.X,
            point.Y,
            0,
            window,
            IntPtr.Zero);
        DestroyMenu(menu);

        if (chosen == 1)
        {
            quit?.Invoke();
        }
    }

    /// <summary>
    /// Say something once. The shell turns this into a notification; clicking it
    /// opens Taskbar settings, which is where the overflow switch lives.
    /// </summary>
    public void Hint(string title, string text)
    {
        if (window == IntPtr.Zero)
        {
            return;
        }

        var data = Data();
        data.uFlags = NIF_INFO;
        data.szInfoTitle = title;
        data.szInfo = text;
        data.dwInfoFlags = NIIF_INFO;
        data.uTimeoutOrVersion = 10000;
        Shell_NotifyIconW(NIM_MODIFY, ref data);

        // The marker is written before the hint is shown, so a shell that refuses
        // it does not bring it back on every start.
    }

    public void Dispose()
    {
        if (window == IntPtr.Zero)
        {
            return;
        }

        var data = Data();
        Shell_NotifyIconW(NIM_DELETE, ref data);
        if (icon != IntPtr.Zero)
        {
            DestroyIcon(icon);
            icon = IntPtr.Zero;
        }
        DestroyWindow(window);
        window = IntPtr.Zero;
    }

    private IntPtr WindowProc(IntPtr hwnd, uint message, IntPtr wparam, IntPtr lparam)
    {
        if (message == CallbackMessage)
        {
            var mouse = (int)(lparam.ToInt64() & 0xffff);
            if (mouse == WM_LBUTTONUP)
            {
                LeftClick?.Invoke();
                return IntPtr.Zero;
            }
            if (mouse == WM_RBUTTONUP)
            {
                ShowMenu();
                return IntPtr.Zero;
            }
            if (mouse == NIN_BALLOONUSERCLICK)
            {
                OpenTaskbarSettings();
                return IntPtr.Zero;
            }
        }
        else if (message == taskbarCreated && taskbarCreated != 0)
        {
            // Explorer restarted and took every icon with it.
            var data = Data();
            Shell_NotifyIconW(NIM_ADD, ref data);
            return IntPtr.Zero;
        }

        return DefWindowProcW(hwnd, message, wparam, lparam);
    }

    private NOTIFYICONDATAW Data() => new()
    {
        cbSize = (uint)Marshal.SizeOf<NOTIFYICONDATAW>(),
        hWnd = window,
        uID = IconId,
        uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP,
        uCallbackMessage = CallbackMessage,
        hIcon = icon,
        szTip = "Radio",
    };

    /// <summary>Where the icon is on screen, or null when the tray will not say.</summary>
    public Windows.Graphics.RectInt32? Bounds()
    {
        if (window == IntPtr.Zero)
        {
            return null;
        }

        // No GUID here either; with guidItem left null the shell uses the
        // window and id, which is what the icon was registered with.
        var identity = new NOTIFYICONIDENTIFIER
        {
            cbSize = (uint)Marshal.SizeOf<NOTIFYICONIDENTIFIER>(),
            hWnd = window,
            uID = IconId,
        };
        return Shell_NotifyIconGetRect(ref identity, out var rect) == 0
            ? new Windows.Graphics.RectInt32(rect.Left, rect.Top, rect.Right - rect.Left, rect.Bottom - rect.Top)
            : null;
    }

    private static void OpenTaskbarSettings()
    {
        try
        {
            System.Diagnostics.Process.Start(new System.Diagnostics.ProcessStartInfo
            {
                FileName = "ms-settings:taskbar",
                UseShellExecute = true,
            });
        }
        catch
        {
            // A locked-down shell may not have a settings protocol; nothing to do.
        }
    }

    private static string IconPath()
    {
        var beside = Path.Combine(AppContext.BaseDirectory, "Assets", "radio.ico");
        if (File.Exists(beside))
        {
            return beside;
        }
        return Path.Combine(AppContext.BaseDirectory, "radio.ico");
    }

    #region Win32

    private delegate IntPtr WndProc(IntPtr hwnd, uint message, IntPtr wparam, IntPtr lparam);

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    private struct WNDCLASSW
    {
        public uint style;
        [MarshalAs(UnmanagedType.FunctionPtr)] public WndProc lpfnWndProc;
        public int cbClsExtra;
        public int cbWndExtra;
        public IntPtr hInstance;
        public IntPtr hIcon;
        public IntPtr hCursor;
        public IntPtr hbrBackground;
        [MarshalAs(UnmanagedType.LPWStr)] public string? lpszMenuName;
        [MarshalAs(UnmanagedType.LPWStr)] public string lpszClassName;
    }

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    private struct NOTIFYICONDATAW
    {
        public uint cbSize;
        public IntPtr hWnd;
        public uint uID;
        public uint uFlags;
        public uint uCallbackMessage;
        public IntPtr hIcon;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 128)] public string szTip;
        public uint dwState;
        public uint dwStateMask;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 256)] public string szInfo;
        public uint uTimeoutOrVersion;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 64)] public string szInfoTitle;
        public uint dwInfoFlags;
        public Guid guidItem;
        public IntPtr hBalloonIcon;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct POINT
    {
        public int X;
        public int Y;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct RECT
    {
        public int Left;
        public int Top;
        public int Right;
        public int Bottom;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct NOTIFYICONIDENTIFIER
    {
        public uint cbSize;
        public IntPtr hWnd;
        public uint uID;
        public Guid guidItem;
    }

    [DllImport("shell32.dll")]
    private static extern int Shell_NotifyIconGetRect(ref NOTIFYICONIDENTIFIER identity, out RECT rect);

    [DllImport("shell32.dll", CharSet = CharSet.Unicode)]
    private static extern bool Shell_NotifyIconW(uint message, ref NOTIFYICONDATAW data);

    [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern ushort RegisterClassW(ref WNDCLASSW windowClass);

    [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern IntPtr CreateWindowExW(
        uint extendedStyle, string className, string windowName, uint style,
        int x, int y, int width, int height,
        IntPtr parent, IntPtr menu, IntPtr instance, IntPtr parameter);

    [DllImport("user32.dll")]
    private static extern bool DestroyWindow(IntPtr window);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern IntPtr DefWindowProcW(IntPtr window, uint message, IntPtr wparam, IntPtr lparam);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern uint RegisterWindowMessageW(string name);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern IntPtr LoadImageW(IntPtr instance, string name, uint type, int width, int height, uint load);

    [DllImport("user32.dll")]
    private static extern bool DestroyIcon(IntPtr icon);

    [DllImport("user32.dll")]
    private static extern int GetSystemMetrics(int index);

    [DllImport("kernel32.dll", CharSet = CharSet.Unicode)]
    private static extern IntPtr GetModuleHandleW(string? name);

    [DllImport("user32.dll")]
    private static extern IntPtr CreatePopupMenu();

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern bool AppendMenuW(IntPtr menu, uint flags, uint id, string text);

    [DllImport("user32.dll")]
    private static extern bool DestroyMenu(IntPtr menu);

    [DllImport("user32.dll")]
    private static extern bool GetCursorPos(out POINT point);

    [DllImport("user32.dll")]
    private static extern bool SetForegroundWindow(IntPtr window);

    [DllImport("user32.dll")]
    private static extern int TrackPopupMenu(
        IntPtr menu, uint flags, int x, int y, int reserved, IntPtr window, IntPtr rect);

    #endregion
}
