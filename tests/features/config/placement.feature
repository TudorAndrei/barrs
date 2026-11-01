Feature: Place items in a bar section
  The placement of an item selects the bar section that shows it.

  Scenario Outline: A known placement selects its section
    Given the config
      """
      return { items = { { id = "clock", placement = "<placement>" } } }
      """
    When barrs loads the config
    Then item "clock" is in the <section> section

    Examples:
      | placement | section |
      | left      | left    |
      | middle    | middle  |
      | center    | middle  |
      | right     | right   |

  Scenario: An item without a placement is in the left section
    Given the config
      """
      return { items = { { id = "clock" } } }
      """
    When barrs loads the config
    Then item "clock" is in the left section

  Scenario: An unknown placement makes the config invalid
    Given the config
      """
      return { items = { { id = "clock", placement = "floating" } } }
      """
    When barrs loads the config
    Then the config is rejected with an error that contains
      | text                  |
      | clock                 |
      | unsupported placement |
      | floating              |
