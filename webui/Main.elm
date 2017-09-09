module Main exposing (..)

import Html exposing (Html, button, div, text, ol, li, b, i, br, code)
import Html.Events exposing (onClick)
import Html.Attributes exposing (..)
import Json.Decode as Decode
import Http
import Task


-- DATA --


type alias Model =
    { graph : Graph
    , err : Maybe String
    }


type alias Ports =
    { input : List Port
    , output : List Port
    }


type alias Port =
    { edge : Maybe Edge
    }


type alias Edge =
    { nodeId : Int
    , portId : Int
    }


type alias Node =
    { id : Int
    , name : String
    , title : String
    , ports : Ports
    , status : NodeStatus
    }


type NodeStatus
    = Stopped
    | Running
    | Paused
    | Dead


type alias NodeType =
    { id : Int
    , name : String
    }


type alias Graph =
    { nodes : List Node
    , types : List NodeType
    }


type Msg
    = Refresh
    | RefreshNodes (Result Http.Error (List Node))
    | UpdateTypes (Result Http.Error (List NodeType))
    | AddNode NodeType
    | AddedNode (Result Http.Error AddNodeResult)


type alias AddNodeResult =
    Result String Int


emptyGraph =
    { nodes = []
    , types = []
    }



-- VIEW --


typesView : Model -> Html Msg
typesView model =
    div []
        [ text "Node types:"
        , ol [] (List.map typeView model.graph.types)
        ]


typeView : NodeType -> Html Msg
typeView node =
    li [] [ button [ onClick (AddNode node) ] [ text node.name ] ]


nodesView : Model -> Html Msg
nodesView model =
    div []
        [ text "Nodes:"
        , ol [] (List.map nodeView model.graph.nodes)
        ]


nodeView : Node -> Html Msg
nodeView node =
    li []
        [ div []
            [ b [] [ text node.name ]
            , text (" : " ++ node.title ++ " (" ++ toString node.status ++ ")")
            , br [] []
            , text "Inputs:"
            , ol [] (List.map portView node.ports.input)
            , text "Outputs:"
            , ol [] (List.map portView node.ports.output)
            ]
        ]


portView : Port -> Html Msg
portView port_ =
    li []
        [ Maybe.withDefault
            (text "Disconnected")
            (Maybe.map edgeView port_.edge)
        ]


edgeView : Edge -> Html Msg
edgeView edge =
    b [] [ text (toString edge.nodeId ++ ":" ++ toString edge.portId) ]


errorView : Model -> Html Msg
errorView model =
    div [ style [ ( "color", "red" ) ] ] [ text (Maybe.withDefault "" model.err) ]



-- DECODE --


decodeNodes : Decode.Decoder (List Node)
decodeNodes =
    Decode.list
        (Decode.map5 Node
            (Decode.field "id" Decode.int)
            (Decode.field "name" Decode.string)
            (Decode.field "title" Decode.string)
            (Decode.field "ports" decodePorts)
            (Decode.field "status" decodeNodeStatus)
        )


decodePorts : Decode.Decoder Ports
decodePorts =
    Decode.map2 Ports
        (Decode.field "in" (Decode.list decodePort))
        (Decode.field "out" (Decode.list decodePort))


decodePort : Decode.Decoder Port
decodePort =
    Decode.map Port
        (Decode.maybe
            (Decode.map2 Edge
                (Decode.at [ "edge", "node" ] Decode.int)
                (Decode.at [ "edge", "port" ] Decode.int)
            )
        )


decodeTypes : Decode.Decoder (List NodeType)
decodeTypes =
    Decode.list
        (Decode.map2 NodeType
            (Decode.field "id" Decode.int)
            (Decode.field "name" Decode.string)
        )


decodeAddNode : Decode.Decoder AddNodeResult
decodeAddNode =
    Decode.andThen
        (\status ->
            if status == "ok" then
                Decode.map Ok (Decode.field "id" Decode.int)
            else
                Decode.succeed (Err status)
        )
        (Decode.field "status" Decode.string)


decodeNodeStatus : Decode.Decoder NodeStatus
decodeNodeStatus =
    Decode.map
        (\msg ->
            case msg of
                "stopped" ->
                    Stopped

                "running" ->
                    Running

                "paused" ->
                    Paused

                _ ->
                    Dead
        )
        Decode.string



-- ACTIONS --


addNode : NodeType -> Cmd Msg
addNode nodeType =
    let
        url =
            "http://localhost:8008/type/" ++ toString nodeType.id ++ "/new"

        request =
            Http.get url decodeAddNode
    in
        Http.send AddedNode request



-- REFRESH --


refreshNodes : Cmd Msg
refreshNodes =
    let
        url =
            "http://localhost:8008/node"

        request =
            Http.get url decodeNodes
    in
        Http.send RefreshNodes request


refreshTypes : Cmd Msg
refreshTypes =
    let
        url =
            "http://localhost:8008/type"

        request =
            Http.get url decodeTypes
    in
        Http.send UpdateTypes request


refresh =
    Cmd.batch [ refreshTypes, refreshNodes ]



-- ERROR --


raiseError : Model -> String -> Model
raiseError model err =
    { model | err = Just err }



-- SPECIAL FUNCTIONS --


init : ( Model, Cmd Msg )
init =
    ( { graph = emptyGraph
      , err = Nothing
      }
    , refresh
    )


update : Msg -> Model -> ( Model, Cmd Msg )
update msg model =
    case msg of
        Refresh ->
            ( model, Cmd.batch [ refreshNodes, refreshTypes ] )

        RefreshNodes (Ok newNodes) ->
            ( { model | graph = { types = model.graph.types, nodes = newNodes } }, Cmd.none )

        RefreshNodes (Err err) ->
            ( raiseError model (toString err), Cmd.none )

        UpdateTypes (Ok newTypes) ->
            ( { model | graph = { types = newTypes, nodes = model.graph.nodes } }, Cmd.none )

        UpdateTypes (Err err) ->
            ( raiseError model (toString err), Cmd.none )

        AddNode nodeType ->
            ( model, addNode nodeType )

        AddedNode (Ok nodeId) ->
            -- TODO show error here if needed (nodeId :: Result)
            ( model, refreshNodes )

        AddedNode (Err err) ->
            ( raiseError model (toString err), Cmd.none )


subscriptions : Model -> Sub Msg
subscriptions model =
    Sub.none


view : Model -> Html Msg
view model =
    div []
        [ errorView model
        , typesView model
        , nodesView model
        ]


main =
    Html.program { init = init, view = view, update = update, subscriptions = subscriptions }
