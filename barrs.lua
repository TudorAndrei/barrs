function open_clock(ctx)
	return true
end

return {
	bar = {
		spacing = 0,
		background = "#000000",
	},
	items = {
		{
			id = "workspaces",
			icon = "󰆍",
			plugin = { kind = "rift_workspaces" },
			hover = { tooltip = "Current Rift workspaces" },
		},
		{
			id = "cpu",
			icon = "󰘚",
			interval = 2,
			plugin = { kind = "cpu" },
			hover = { tooltip = "CPU usage" },
		},
		{
			id = "gpu",
			icon = "󰢮",
			interval = 2,
			plugin = { kind = "gpu" },
			hover = { tooltip = "GPU usage" },
		},
		{
			id = "battery",
			icon = "󰂄",
			interval = 10,
			plugin = { kind = "battery" },
			hover = { tooltip = "Battery status" },
		},
		{
			id = "time",
			icon = "󰥔",
			interval = 1,
			plugin = { kind = "time" },
			handlers = { click = "open_clock" },
			hover = { tooltip = "Current time" },
		},
	},
}
