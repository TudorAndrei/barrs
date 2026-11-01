Feature: Load the Lua config
  A barrs config is a Lua file. It returns the bar settings and the list of
  bar items. barrs loads the config once and rejects it when it is not valid.

  Scenario: An empty bar table uses the notch defaults
    Given the config
      """
      return {
        bar = {},
        items = { { id = "clock", plugin = { kind = "time" } } },
      }
      """
    When barrs loads the config
    Then the config has 1 item
    And the bar settings are
      | setting              | value |
      | spacing              | 6     |
      | notch_width          | 200   |
      | notch_padding        | 8     |
      | notch_offset         | 0     |
      | notch_display_height | 0     |
    And the bar has no background

  Scenario: The bar table overrides the notch settings
    Given the config
      """
      return {
        bar = {
          notch_width = 240,
          notch_padding = 12,
          notch_offset = 3,
          notch_display_height = 32,
        },
        items = { { id = "clock", plugin = { kind = "time" } } },
      }
      """
    When barrs loads the config
    Then the bar settings are
      | setting              | value |
      | notch_width          | 240   |
      | notch_padding        | 12    |
      | notch_offset         | 3     |
      | notch_display_height | 32    |

  Scenario: Built-in plugins become system item sources
    Given the config
      """
      return {
        items = {
          { id = "clock", plugin = { kind = "time" } },
          { id = "date", plugin = { kind = "date", format = "%d/%m" } },
        },
      }
      """
    When barrs loads the config
    Then item "clock" shows the time
    And item "date" shows the date in the format "%d/%m"
