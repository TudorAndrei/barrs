Feature: Connect items to Lua functions
  An item names global Lua functions for its handlers and for its update.
  barrs checks that each named function exists when it loads the config.

  Scenario: A Lua plugin uses its update function as the item source
    Given the config
      """
      function memory_usage(ctx) return "42%" end
      return {
        items = { { id = "memory", plugin = { kind = "lua", update = "memory_usage" } } },
      }
      """
    When barrs loads the config
    Then item "memory" is updated by the Lua function "memory_usage"

  Scenario: A Lua plugin must name an update function
    Given the config
      """
      return { items = { { id = "memory", plugin = { kind = "lua" } } } }
      """
    When barrs loads the config
    Then the config is rejected with an error that contains
      | text            |
      | must set update |

  Scenario: The update function of a Lua plugin must exist
    Given the config
      """
      return {
        items = { { id = "memory", plugin = { kind = "lua", update = "missing" } } },
      }
      """
    When barrs loads the config
    Then the config is rejected with an error that contains
      | text                            |
      | missing update function missing |

  Scenario: A handler must name a function that exists
    Given the config
      """
      return {
        items = {
          {
            id = "clock",
            plugin = { kind = "time" },
            handlers = { click = "missing_handler" },
          },
        },
      }
      """
    When barrs loads the config
    Then the config is rejected with an error that contains
      | text            |
      | missing handler |
