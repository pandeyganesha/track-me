import Chart from 'chart.js/auto';

const API_BASE = 'http://127.0.0.1:3000/api';

// DOM Elements
const currentAppEl = document.getElementById('current-app');
const currentTitleEl = document.getElementById('current-title');
const statusIndicator = document.querySelector('.status-indicator');
const statsListEl = document.getElementById('stats-list');
const totalTimeEl = document.getElementById('total-time');

let usageChart = null;

// Format milliseconds to pretty string (e.g., "1h 45m")
function formatDuration(ms) {
    const totalSeconds = Math.floor(ms / 1000);
    const hours = Math.floor(totalSeconds / 3600);
    const minutes = Math.floor((totalSeconds % 3600) / 60);
    const seconds = totalSeconds % 60;

    if (hours > 0) return `${hours}h ${minutes}m`;
    if (minutes > 0) return `${minutes}m ${seconds}s`;
    return `${seconds}s`;
}

// Generate beautiful vibrant colors for the chart
function generateColors(count) {
    const colors = [
        '#3b82f6', // blue
        '#8b5cf6', // violet
        '#ec4899', // pink
        '#f43f5e', // rose
        '#f59e0b', // amber
        '#10b981', // emerald
        '#06b6d4', // cyan
        '#6366f1', // indigo
        '#a855f7', // purple
        '#14b8a6'  // teal
    ];
    return Array.from({ length: count }, (_, i) => colors[i % colors.length]);
}

async function fetchStatus() {
    try {
        const res = await fetch(`${API_BASE}/status`);
        const data = await res.json();
        
        if (data) {
            const isIdle = data.is_idle;
            const className = data.class || 'Desktop';
            const title = data.title || '';
            
            if (isIdle) {
                statusIndicator.classList.add('idle');
                currentAppEl.classList.add('idle-text');
                currentAppEl.textContent = 'Idle (Away)';
                currentTitleEl.textContent = 'Tracking paused automatically';
            } else {
                statusIndicator.classList.remove('idle');
                currentAppEl.classList.remove('idle-text');
                currentAppEl.textContent = className;
                currentTitleEl.textContent = title || 'Active';
            }
        }
    } catch (e) {
        currentAppEl.textContent = 'Offline';
        currentTitleEl.textContent = 'Cannot connect to API server';
        statusIndicator.style.background = '#ef4444';
        statusIndicator.style.boxShadow = '0 0 10px #ef4444';
    }
}

async function fetchToday() {
    try {
        const res = await fetch(`${API_BASE}/today`);
        const data = await res.json();
        
        // Sort data by duration descending
        const sortedData = Object.entries(data)
            .sort((a, b) => b[1] - a[1])
            .filter(([app, ms]) => ms > 1000); // Only show apps used > 1 second

        updateUI(sortedData);
    } catch (e) {
        console.error('Failed to fetch today stats:', e);
    }
}

function updateUI(data) {
    const apps = data.map(d => d[0]);
    const durations = data.map(d => d[1]);
    const totalMs = durations.reduce((a, b) => a + b, 0);
    const colors = generateColors(apps.length);

    // Update Total Time
    totalTimeEl.textContent = formatDuration(totalMs);

    // Update Stats List
    statsListEl.innerHTML = '';
    data.forEach(([app, ms], i) => {
        const item = document.createElement('div');
        item.className = 'stat-item';
        item.innerHTML = `
            <div class="app-info">
                <div class="app-color" style="background: ${colors[i]}"></div>
                <div class="app-name">${app}</div>
            </div>
            <div class="app-time">${formatDuration(ms)}</div>
        `;
        statsListEl.appendChild(item);
    });

    // Update Chart
    const ctx = document.getElementById('usageChart').getContext('2d');
    
    if (usageChart) {
        usageChart.data.labels = apps;
        usageChart.data.datasets[0].data = durations;
        usageChart.data.datasets[0].backgroundColor = colors;
        usageChart.update();
    } else {
        Chart.defaults.color = '#94a3b8';
        Chart.defaults.font.family = "'Outfit', sans-serif";
        
        usageChart = new Chart(ctx, {
            type: 'doughnut',
            data: {
                labels: apps,
                datasets: [{
                    data: durations,
                    backgroundColor: colors,
                    borderWidth: 2,
                    borderColor: '#1e293b', // Matches glass background
                    hoverOffset: 10,
                    borderRadius: 4
                }]
            },
            options: {
                responsive: true,
                maintainAspectRatio: false,
                cutout: '75%', // Thinner ring for premium look
                layout: {
                    padding: 20
                },
                plugins: {
                    legend: {
                        position: 'right',
                        labels: {
                            padding: 20,
                            usePointStyle: true,
                            pointStyle: 'circle',
                            font: { size: 14 }
                        }
                    },
                    tooltip: {
                        backgroundColor: 'rgba(15, 23, 42, 0.9)',
                        titleFont: { size: 14, family: "'Outfit', sans-serif" },
                        bodyFont: { size: 14, family: "'Outfit', sans-serif" },
                        padding: 15,
                        cornerRadius: 12,
                        callbacks: {
                            label: function(context) {
                                return ` ${formatDuration(context.raw)}`;
                            }
                        }
                    }
                }
            }
        });
    }
}

// Initial fetch
fetchStatus();
fetchToday();

// Poll every 1 second for status, 5 seconds for chart
setInterval(fetchStatus, 1000);
setInterval(fetchToday, 5000);
