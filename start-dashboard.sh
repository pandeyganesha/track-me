#!/bin/bash

echo "Starting track-me-api on http://localhost:3000..."
# Start the API server in the background
~/.cargo/bin/track-me-api &
API_PID=$!

echo "Starting Vite Dashboard on http://localhost:5173..."
# Start the Vite frontend
cd dashboard && npm run dev &
VITE_PID=$!

echo "------------------------------------------------"
echo "✅ Dashboard is running! Open your browser to:"
echo "👉 http://localhost:5173"
echo "------------------------------------------------"
echo "(Press Ctrl+C here to stop the dashboard)"

# Wait for user to press Ctrl+C
trap "echo 'Stopping servers...'; kill $API_PID $VITE_PID; exit" INT TERM
wait
