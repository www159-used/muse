Feature: Default all-local playlist playback

  Scenario: Playing 全部本地 starts sequential playback of imported tracks
    Given "downloads/a.mp3" is a copy of the fixture "silence.mp3"
    And "downloads/b.flac" is a copy of the fixture "tone.flac"

    When I import the folder "downloads"
    And I play the playlist "全部本地"

    Then the import finishes without errors
    And the current playlist is "全部本地"
    And the playback mode is "sequential"
    And the current track index is 1 of 2
