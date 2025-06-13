import classNames from 'classnames';
import { useState } from 'react';
import { FiMonitor, FiMoon, FiSun } from 'react-icons/fi';

export const updateTheme = () => {
    const theme = localStorage.getItem('color-theme') || 'system';

    document.documentElement.classList.remove('light', 'dark', 'system');
    document.documentElement.classList.add(theme);

    // Update theme-color meta tag for Apple's overscroll
    let metaThemeColor = document.querySelector('meta[name="theme-color"]') as HTMLMetaElement;

    if (!metaThemeColor) {
        metaThemeColor = document.createElement('meta');
        metaThemeColor.name = 'theme-color';
        document.head.appendChild(metaThemeColor);
    }

    // Define theme colors
    const themeColors = {
        light: 'rgb(253, 246, 227)', // --theme-bg-primary for light
        dark: 'rgb(0, 0, 0)', // --theme-bg-primary for dark
    };

    // Determine the actual theme to apply
    let actualTheme = theme;

    if (theme === 'system') {
        actualTheme = window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
    }

    // Set the theme color
    metaThemeColor.content = themeColors[actualTheme as keyof typeof themeColors];
};

export const ThemeSwitcher = () => {
    const theme = localStorage.getItem('color-theme') || 'system';
    const [currentTheme, setCurrentTheme] = useState(theme);
    const setTheme = (theme: string) => {
        localStorage.setItem('color-theme', theme);
        setCurrentTheme(theme);

        updateTheme();
    };

    return (
        <div className="flex h-8 gap-1 p-0.5">
            {(
                [
                    ['light', <FiSun key="light" />],
                    ['dark', <FiMoon key="dark" />],
                    ['system', <FiMonitor key="system" />],
                ] as const
            ).map(([theme, icon]) => (
                <button
                    key={theme}
                    onClick={(event) => {
                        event.preventDefault();
                        event.stopPropagation();
                        setTheme(theme);
                    }}
                    className={classNames(
                        currentTheme === theme && 'bg-primary',
                        'h-full flex items-center justify-center aspect-square button'
                    )}
                >
                    {icon}
                </button>
            ))}
        </div>
    );
};
